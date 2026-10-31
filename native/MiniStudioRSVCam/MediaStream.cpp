#include "MediaStream.h"
#include "SharedFrame.h"
#include <thread>

using Microsoft::WRL::Wrappers::SRWLock;

static const UINT32 kWidth = MSVCAM_WIDTH;
static const UINT32 kHeight = MSVCAM_HEIGHT;
static const LONGLONG kFrameDuration = 10000000LL * MSVCAM_FPS_DEN / MSVCAM_FPS_NUM; // 100ns

MediaStream::MediaStream()
{
    g_objectCount.fetch_add(1);
}

MediaStream::~MediaStream()
{
    g_objectCount.fetch_sub(1);
}

HRESULT MediaStream::Initialize(IMFMediaSource* source, int index, const wchar_t* framePath)
{
    _source = source;
    _framePath = framePath ? framePath : L"";
    _index = index;

    HRESULT hr = MFCreateAttributes(&_attributes, 8);
    if (FAILED(hr)) return hr;

    _attributes->SetGUID(MF_DEVICESTREAM_STREAM_CATEGORY, PINNAME_VIDEO_CAPTURE);
    _attributes->SetUINT32(MF_DEVICESTREAM_STREAM_ID, index);
    _attributes->SetUINT32(MF_DEVICESTREAM_FRAMESERVER_SHARED, 1);
    _attributes->SetUINT32(MF_DEVICESTREAM_ATTRIBUTE_FRAMESOURCE_TYPES, MFFrameSourceTypes_Color);

    hr = MFCreateEventQueue(&_queue);
    if (FAILED(hr)) return hr;

    // Tipo de mídia único: NV12 1280x720@30, BT.709 faixa limitada (casa com o produtor Rust).
    ComPtr<IMFMediaType> nv12;
    hr = MFCreateMediaType(&nv12);
    if (FAILED(hr)) return hr;

    nv12->SetGUID(MF_MT_MAJOR_TYPE, MFMediaType_Video);
    nv12->SetGUID(MF_MT_SUBTYPE, MFVideoFormat_NV12);
    nv12->SetUINT32(MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive);
    nv12->SetUINT32(MF_MT_ALL_SAMPLES_INDEPENDENT, TRUE);
    MFSetAttributeSize(nv12.Get(), MF_MT_FRAME_SIZE, kWidth, kHeight);
    nv12->SetUINT32(MF_MT_DEFAULT_STRIDE, kWidth);
    MFSetAttributeRatio(nv12.Get(), MF_MT_FRAME_RATE, MSVCAM_FPS_NUM, MSVCAM_FPS_DEN);
    MFSetAttributeRatio(nv12.Get(), MF_MT_PIXEL_ASPECT_RATIO, 1, 1);
    nv12->SetUINT32(MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235);
    nv12->SetUINT32(MF_MT_YUV_MATRIX, MFVideoTransferMatrix_BT709);

    IMFMediaType* types[] = { nv12.Get() };
    hr = MFCreateStreamDescriptor(_index, 1, types, &_descriptor);
    if (FAILED(hr)) return hr;

    ComPtr<IMFMediaTypeHandler> handler;
    hr = _descriptor->GetMediaTypeHandler(&handler);
    if (FAILED(hr)) return hr;

    return handler->SetCurrentMediaType(nv12.Get());
}

HRESULT MediaStream::GetStreamDescriptor(IMFStreamDescriptor** ppDescriptor)
{
    if (!ppDescriptor) return E_POINTER;
    if (!_descriptor) return MF_E_NOT_INITIALIZED;
    return _descriptor.CopyTo(ppDescriptor);
}

HRESULT MediaStream::Start(IMFMediaType* type)
{
    auto lock = _lock.LockExclusive();
    if (!_queue || !_allocator) return MF_E_SHUTDOWN;

    // Chamado via SetStreamState(RUNNING) pode vir sem tipo: usa o tipo atual do descritor.
    ComPtr<IMFMediaType> current;
    if (!type && _descriptor)
    {
        ComPtr<IMFMediaTypeHandler> handler;
        if (SUCCEEDED(_descriptor->GetMediaTypeHandler(&handler)))
            handler->GetCurrentMediaType(&current);
        type = current.Get();
    }

    if (type)
        type->GetGUID(MF_MT_SUBTYPE, &_format);

    _nextFrame = std::chrono::steady_clock::now();
    _reader.Open(_framePath.empty() ? nullptr : _framePath.c_str());
    _scratch.resize(MSVCAM_FRAME_BYTES);

    HRESULT hr = _allocator->InitializeSampleAllocator(10, type);
    if (FAILED(hr)) return hr;

    hr = _queue->QueueEventParamVar(MEStreamStarted, GUID_NULL, S_OK, nullptr);
    _state = MF_STREAM_STATE_RUNNING;
    return hr;
}

HRESULT MediaStream::Stop()
{
    auto lock = _lock.LockExclusive();
    if (!_queue) return MF_E_SHUTDOWN;

    if (_allocator)
        _allocator->UninitializeSampleAllocator();

    HRESULT hr = _queue->QueueEventParamVar(MEStreamStopped, GUID_NULL, S_OK, nullptr);
    _state = MF_STREAM_STATE_STOPPED;
    return hr;
}

HRESULT MediaStream::Shutdown()
{
    auto lock = _lock.LockExclusive();
    if (_queue)
    {
        _queue->Shutdown();
        _queue.Reset();
    }
    _allocator.Reset();
    _descriptor.Reset();
    _attributes.Reset();
    _source = nullptr;
    _reader.Close();
    return S_OK;
}

HRESULT MediaStream::SetAllocator(IUnknown* allocator)
{
    if (!allocator) return E_POINTER;
    _allocator.Reset();
    return allocator->QueryInterface(IID_PPV_ARGS(&_allocator));
}

// ------------------------------------------------------------ IMFMediaEventGenerator

STDMETHODIMP MediaStream::BeginGetEvent(IMFAsyncCallback* cb, IUnknown* state)
{
    auto lock = _lock.LockExclusive();
    if (!_queue) return MF_E_SHUTDOWN;
    return _queue->BeginGetEvent(cb, state);
}

STDMETHODIMP MediaStream::EndGetEvent(IMFAsyncResult* result, IMFMediaEvent** ev)
{
    auto lock = _lock.LockExclusive();
    if (!_queue) return MF_E_SHUTDOWN;
    return _queue->EndGetEvent(result, ev);
}

STDMETHODIMP MediaStream::GetEvent(DWORD flags, IMFMediaEvent** ev)
{
    ComPtr<IMFMediaEventQueue> q;
    {
        auto lock = _lock.LockExclusive();
        if (!_queue) return MF_E_SHUTDOWN;
        q = _queue;
    }
    return q->GetEvent(flags, ev);
}

STDMETHODIMP MediaStream::QueueEvent(MediaEventType met, REFGUID guid, HRESULT status, const PROPVARIANT* pv)
{
    auto lock = _lock.LockExclusive();
    if (!_queue) return MF_E_SHUTDOWN;
    return _queue->QueueEventParamVar(met, guid, status, pv);
}

// ------------------------------------------------------------ IMFMediaStream / 2

STDMETHODIMP MediaStream::GetMediaSource(IMFMediaSource** ppMediaSource)
{
    auto lock = _lock.LockExclusive();
    if (!ppMediaSource) return E_POINTER;
    if (!_source) return MF_E_SHUTDOWN;
    _source->AddRef();
    *ppMediaSource = _source;
    return S_OK;
}

STDMETHODIMP MediaStream::RequestSample(IUnknown* pToken)
{
    auto lock = _lock.LockExclusive();
    if (!_allocator || !_queue) return MF_E_SHUTDOWN;
    if (_state != MF_STREAM_STATE_RUNNING) return MF_E_INVALIDREQUEST;
    // Cadência do consumidor independente dos eventos da captura e de imagens estáticas.
    std::this_thread::sleep_until(_nextFrame);
    const auto now = std::chrono::steady_clock::now();
    const auto period = std::chrono::nanoseconds(1000000000 / MSVCAM_FPS_NUM);
    _nextFrame += period;
    if (_nextFrame < now) _nextFrame = now + period;

    ComPtr<IMFSample> sample;
    HRESULT hr = _allocator->AllocateSample(&sample);
    if (FAILED(hr)) return hr;

    sample->SetSampleTime(MFGetSystemTime());
    sample->SetSampleDuration(kFrameDuration);

    hr = FillSample(sample.Get());
    if (FAILED(hr)) return hr;

    if (pToken)
        sample->SetUnknown(MFSampleExtension_Token, pToken);

    return _queue->QueueEventParamUnk(MEMediaSample, GUID_NULL, S_OK, sample.Get());
}

STDMETHODIMP MediaStream::SetStreamState(MF_STREAM_STATE value)
{
    switch (value)
    {
    case MF_STREAM_STATE_RUNNING:
        return Start(nullptr);
    case MF_STREAM_STATE_STOPPED:
        return Stop();
    default:
        return E_INVALIDARG;
    }
}

STDMETHODIMP MediaStream::GetStreamState(MF_STREAM_STATE* value)
{
    auto lock = _lock.LockExclusive();
    if (!value) return E_POINTER;
    *value = _state;
    return S_OK;
}

// ------------------------------------------------------------ preenchimento do frame

HRESULT MediaStream::FillSample(IMFSample* sample)
{
    ComPtr<IMFMediaBuffer> buffer;
    HRESULT hr = sample->GetBufferByIndex(0, &buffer);
    if (FAILED(hr)) return hr;

    _reader.Open(_framePath.empty() ? nullptr : _framePath.c_str());
    bool haveFrame = _reader.ReadLatest(_scratch.data(), (uint32_t)_scratch.size());

    ComPtr<IMF2DBuffer2> buffer2D;
    if (SUCCEEDED(buffer.As(&buffer2D)))
    {
        BYTE* scanline = nullptr; LONG pitch = 0; BYTE* start = nullptr; DWORD length = 0;
        hr = buffer2D->Lock2DSize(MF2DBuffer_LockFlags_Write, &scanline, &pitch, &start, &length);
        if (FAILED(hr)) return hr;

        if (pitch < static_cast<LONG>(kWidth) || scanline < start ||
            static_cast<size_t>(scanline - start) + static_cast<size_t>(pitch) * (kHeight + kHeight / 2 - 1) + kWidth > length) {
            buffer2D->Unlock2D(); return MF_E_BUFFERTOOSMALL;
        }
        if (haveFrame)
        {
            const BYTE* srcY = _scratch.data();
            const BYTE* srcUV = srcY + (size_t)kWidth * kHeight;
            for (UINT32 y = 0; y < kHeight; y++)
                memcpy(scanline + (size_t)y * pitch, srcY + (size_t)y * kWidth, kWidth);
            for (UINT32 r = 0; r < kHeight / 2; r++)
                memcpy(scanline + (size_t)(kHeight + r) * pitch, srcUV + (size_t)r * kWidth, kWidth);
        }
        else
        {
            FillPlaceholder(scanline, pitch);
        }

        buffer2D->Unlock2D();
        return buffer->SetCurrentLength(MSVCAM_FRAME_BYTES);
    }

    // Fallback 1D contíguo (pitch = largura).
    BYTE* data = nullptr; DWORD maxLen = 0;
    hr = buffer->Lock(&data, &maxLen, nullptr);
    if (FAILED(hr)) return hr;

    if (maxLen < MSVCAM_FRAME_BYTES) { buffer->Unlock(); return MF_E_BUFFERTOOSMALL; }
    if (haveFrame)
        memcpy(data, _scratch.data(), MSVCAM_FRAME_BYTES);
    else
        FillPlaceholder(data, kWidth);

    buffer->SetCurrentLength(MSVCAM_FRAME_BYTES);
    buffer->Unlock();
    return S_OK;
}

// Quadro neutro de "sem sinal" (cinza escuro): Y baixo, croma neutra (128).
void MediaStream::FillPlaceholder(BYTE* dst, LONG pitch)
{
    for (UINT32 y = 0; y < kHeight; y++)
        memset(dst + (size_t)y * pitch, 30, kWidth);
    for (UINT32 r = 0; r < kHeight / 2; r++)
        memset(dst + (size_t)(kHeight + r) * pitch, 128, kWidth);
}
