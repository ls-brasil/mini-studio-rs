#include "FrameWriter.h"
#include "SharedFrame.h"
#include <string>

HRESULT FrameWriter::Open(const wchar_t* testPath)
{
    if (_view) return S_OK;
    std::wstring path;
    if (testPath) path = testPath;
    else if (!FramePath(path)) return E_FAIL;
    _file = CreateFileW(path.c_str(), GENERIC_READ | GENERIC_WRITE,
        FILE_SHARE_READ | FILE_SHARE_WRITE, nullptr, OPEN_ALWAYS, FILE_ATTRIBUTE_NORMAL, nullptr);
    if (_file == INVALID_HANDLE_VALUE) return HRESULT_FROM_WIN32(GetLastError());
    // O byte 1 reserva o produtor durante toda a sessão, inclusive entre processos.
    OVERLAPPED owner{}; owner.Offset = 1;
    if (!LockFileEx(_file, LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY, 0, 1, 0, &owner)) {
        HRESULT hr = HRESULT_FROM_WIN32(GetLastError()); Close(); return hr;
    }
    _mapping = CreateFileMappingW(_file, nullptr, PAGE_READWRITE, 0, MSVCAM_TOTAL_BYTES, nullptr);
    if (!_mapping) { HRESULT hr = HRESULT_FROM_WIN32(GetLastError()); Close(); return hr; }
    _view = static_cast<uint8_t*>(MapViewOfFile(_mapping, FILE_MAP_ALL_ACCESS, 0, 0, MSVCAM_TOTAL_BYTES));
    if (!_view) { HRESULT hr = HRESULT_FROM_WIN32(GetLastError()); Close(); return hr; }
    FrameLock lock(_file, true);
    if (!lock.ok()) { HRESULT hr = HRESULT_FROM_WIN32(GetLastError()); Close(); return hr; }
    memset(_view, 0, MSVCAM_TOTAL_BYTES);
    auto* h = reinterpret_cast<MsVCamHeader*>(_view);
    h->magic = MSVCAM_MAGIC; h->version = MSVCAM_VERSION;
    h->width = MSVCAM_WIDTH; h->height = MSVCAM_HEIGHT;
    h->frameBytes = MSVCAM_FRAME_BYTES;
    return S_OK;
}

HRESULT FrameWriter::Publish(const uint8_t* pixels, uint32_t bytes)
{
    if (!_view) return E_UNEXPECTED;
    if (!pixels || bytes != MSVCAM_FRAME_BYTES) return E_INVALIDARG;
    FrameLock lock(_file, true);
    if (!lock.ok()) return HRESULT_FROM_WIN32(GetLastError());
    memcpy(_view + MSVCAM_HEADER_BYTES, pixels, bytes);
    auto* h = reinterpret_cast<MsVCamHeader*>(_view);
    LARGE_INTEGER now{}; QueryPerformanceCounter(&now);
    h->producerQpc = now.QuadPart;
    ++h->sequence;
    return S_OK;
}

void FrameWriter::Close()
{
    if (_view) {
        {
            FrameLock lock(_file, true);
            if (lock.ok()) {
                auto* h = reinterpret_cast<MsVCamHeader*>(_view);
                h->producerQpc = 0;
                // Não deixa o último vídeo persistido após encerrar a saída.
                memset(_view + MSVCAM_HEADER_BYTES, 0, MSVCAM_FRAME_BYTES);
            }
        }
        UnmapViewOfFile(_view); _view = nullptr;
    }
    if (_mapping) { CloseHandle(_mapping); _mapping = nullptr; }
    if (_file != INVALID_HANDLE_VALUE) { CloseHandle(_file); _file = INVALID_HANDLE_VALUE; }
}
