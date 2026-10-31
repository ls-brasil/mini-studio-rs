#include "MediaSource.h"
#include "SharedFrame.h"
#include "FrameWriter.h"
#include <mfreadwrite.h>
#include <chrono>
#include <stdio.h>

std::atomic<long> g_objectCount{0};
#define CHECK_HR(call) do { HRESULT hr=(call); if (FAILED(hr)) { fprintf(stderr,"%s: 0x%08lX (linha %d)\n",#call,hr,__LINE__); return 1; } } while (0)
#define CHECK(value) do { if (!(value)) { fprintf(stderr,"Falha: %s (linha %d)\n",#value,__LINE__); return 1; } } while (0)

int main()
{
    CHECK_HR(CoInitializeEx(nullptr, COINIT_MULTITHREADED));
    CHECK_HR(MFStartup(MF_VERSION));
    wchar_t folder[MAX_PATH], path[MAX_PATH];
    CHECK(GetTempPathW(MAX_PATH,folder)); CHECK(GetTempFileNameW(folder,L"mrm",0,path));
    {
        FrameWriter writer;
        CHECK_HR(writer.Open(path));
        std::vector<uint8_t> pixels(MSVCAM_FRAME_BYTES,128);
        auto source=Make<MediaSource>();
        CHECK_HR(source->Initialize(nullptr,path));
        ComPtr<IMFVideoSampleAllocatorEx> allocator;
        CHECK_HR(MFCreateVideoSampleAllocatorEx(IID_PPV_ARGS(&allocator)));
        CHECK_HR(source->SetDefaultAllocator(0,allocator.Get()));
        ComPtr<IMFSourceReader> reader;
        CHECK_HR(MFCreateSourceReaderFromMediaSource(source.Get(),nullptr,&reader));
        ComPtr<IMFMediaType> type;
        CHECK_HR(reader->GetNativeMediaType(0,0,&type));
        GUID subtype{}; CHECK_HR(type->GetGUID(MF_MT_SUBTYPE,&subtype));
        UINT32 width=0,height=0,num=0,den=0;
        CHECK_HR(MFGetAttributeSize(type.Get(),MF_MT_FRAME_SIZE,&width,&height));
        CHECK_HR(MFGetAttributeRatio(type.Get(),MF_MT_FRAME_RATE,&num,&den));
        CHECK(subtype==MFVideoFormat_NV12 && width==1280 && height==720 && num==30 && den==1);
        CHECK_HR(reader->SetCurrentMediaType(0,nullptr,type.Get()));
        LONGLONG previous=0;
        const auto start=std::chrono::steady_clock::now();
        for (int i=0;i<31;i++) {
            const uint8_t luma=i<15 ? 50 : 90;
            memset(pixels.data(),luma,MSVCAM_WIDTH*MSVCAM_HEIGHT);
            CHECK_HR(writer.Publish(pixels.data(),static_cast<uint32_t>(pixels.size())));
            DWORD flags=0; LONGLONG timestamp=0; ComPtr<IMFSample> sample;
            CHECK_HR(reader->ReadSample(0,0,nullptr,&flags,&timestamp,&sample));
            CHECK(sample && !(flags & MF_SOURCE_READERF_ERROR));
            CHECK(i==0 || timestamp>previous); previous=timestamp;
            LONGLONG duration=0; CHECK_HR(sample->GetSampleDuration(&duration));
            CHECK(duration==10000000LL/30);
            ComPtr<IMFMediaBuffer> buffer; CHECK_HR(sample->ConvertToContiguousBuffer(&buffer));
            DWORD length=0; CHECK_HR(buffer->GetCurrentLength(&length));
            CHECK(length>=MSVCAM_FRAME_BYTES);
            BYTE* data=nullptr; CHECK_HR(buffer->Lock(&data,nullptr,nullptr));
            const auto y=data[0];
            CHECK(y==50 || y==90);
            for (unsigned j=0;j<MSVCAM_WIDTH*MSVCAM_HEIGHT;j++) CHECK(data[j]==y);
            for (unsigned j=MSVCAM_WIDTH*MSVCAM_HEIGHT;j<MSVCAM_FRAME_BYTES;j++) CHECK(data[j]==128);
            CHECK(i!=30 || y==90);
            CHECK_HR(buffer->Unlock());
        }
        const double elapsed=std::chrono::duration<double>(std::chrono::steady_clock::now()-start).count();
        CHECK(elapsed>=0.9 && elapsed<1.15);
        printf("Media Foundation: 31 amostras NV12, 720p, %.2f FPS; timestamps e buffers OK.\n",30.0/elapsed);
        CHECK_HR(reader->Flush(MF_SOURCE_READER_ALL_STREAMS));
        reader.Reset();
        CHECK_HR(source->Shutdown());
    }
    CHECK(DeleteFileW(path));
    CHECK_HR(MFShutdown()); CoUninitialize();
    CHECK(g_objectCount==0);
    return 0;
}
