#include "FrameReader.h"
#include "SharedFrame.h"

bool FrameReader::Open(const wchar_t* testPath)
{
    if (_view) return true;
    LARGE_INTEGER freq{}; QueryPerformanceFrequency(&freq); _qpcFreq = freq.QuadPart;
    std::wstring path;
    if (testPath) path = testPath;
    else if (!FramePath(path)) return false;
    _file = CreateFileW(path.c_str(), GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE,
        nullptr, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, nullptr);
    if (_file == INVALID_HANDLE_VALUE) return false;
    LARGE_INTEGER size{};
    if (!GetFileSizeEx(_file, &size) || size.QuadPart != MSVCAM_TOTAL_BYTES) { Close(); return false; }
    _mapping = CreateFileMappingW(_file, nullptr, PAGE_READONLY, 0, 0, nullptr);
    if (!_mapping) { Close(); return false; }
    _view = static_cast<const uint8_t*>(MapViewOfFile(_mapping, FILE_MAP_READ, 0, 0, MSVCAM_TOTAL_BYTES));
    if (!_view) { Close(); return false; }
    return true;
}

void FrameReader::Close()
{
    if (_view) { UnmapViewOfFile(_view); _view = nullptr; }
    if (_mapping) { CloseHandle(_mapping); _mapping = nullptr; }
    if (_file != INVALID_HANDLE_VALUE) { CloseHandle(_file); _file = INVALID_HANDLE_VALUE; }
}

bool FrameReader::ReadLatest(uint8_t* dst, uint32_t bytes)
{
    if (!_view || !dst || bytes < MSVCAM_FRAME_BYTES) return false;
    FrameLock lock(_file, false);
    if (!lock.ok()) return false;
    auto* h = reinterpret_cast<const MsVCamHeader*>(_view);
    if (h->magic != MSVCAM_MAGIC || h->version != MSVCAM_VERSION || h->width != MSVCAM_WIDTH ||
        h->height != MSVCAM_HEIGHT || h->frameBytes != MSVCAM_FRAME_BYTES || h->producerQpc == 0) return false;
    LARGE_INTEGER now{}; QueryPerformanceCounter(&now);
    const int64_t age = now.QuadPart - h->producerQpc;
    if (_qpcFreq <= 0 || age < 0 || age > _qpcFreq / 2) return false;
    memcpy(dst, _view + MSVCAM_HEADER_BYTES, MSVCAM_FRAME_BYTES);
    return true;
}
