#pragma once
#include <windows.h>
#include <stdint.h>
class FrameReader {
public:
    ~FrameReader() { Close(); }
    bool Open(const wchar_t* testPath = nullptr);
    void Close();
    bool ReadLatest(uint8_t* dst, uint32_t bytes);
private:
    HANDLE _file = INVALID_HANDLE_VALUE;
    HANDLE _mapping = nullptr;
    const uint8_t* _view = nullptr;
    int64_t _qpcFreq = 0;
};
