#pragma once
#include <windows.h>
#include <stdint.h>

class FrameWriter {
public:
    ~FrameWriter() { Close(); }
    HRESULT Open(const wchar_t* testPath = nullptr);
    HRESULT Publish(const uint8_t* pixels, uint32_t bytes);
    void Close();
private:
    HANDLE _file = INVALID_HANDLE_VALUE;
    HANDLE _mapping = nullptr;
    uint8_t* _view = nullptr;
};
