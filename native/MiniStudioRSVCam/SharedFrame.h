// Transporte privado desta implementação. O Rust publica NV12 pela DLL; somente C++ mapeia o arquivo.
#pragma once
#include <windows.h>
#include <shlobj.h>
#include <stdint.h>
#include <string>

#define MSVCAM_MAGIC 0x3252534Du
#define MSVCAM_VERSION 2u
#define MSVCAM_WIDTH 1280u
#define MSVCAM_HEIGHT 720u
#define MSVCAM_FPS_NUM 30u
#define MSVCAM_FPS_DEN 1u
#define MSVCAM_FRAME_BYTES (MSVCAM_WIDTH * MSVCAM_HEIGHT * 3u / 2u)
#define MSVCAM_HEADER_BYTES 64u
#define MSVCAM_TOTAL_BYTES (MSVCAM_HEADER_BYTES + MSVCAM_FRAME_BYTES)

struct MsVCamHeader {
    uint32_t magic, version, width, height, frameBytes, reserved;
    uint64_t sequence;
    int64_t producerQpc;
    uint32_t padding[6];
};
static_assert(sizeof(MsVCamHeader) == MSVCAM_HEADER_BYTES);
static_assert(offsetof(MsVCamHeader, producerQpc) == 32);

inline bool FramePath(std::wstring& out) {
    PWSTR base = nullptr;
    if (FAILED(SHGetKnownFolderPath(FOLDERID_ProgramData, 0, nullptr, &base))) return false;
    out.assign(base); CoTaskMemFree(base);
    out += L"\\MiniStudioRS\\vcam-frames.bin";
    return true;
}

// Locks explícitos nos dois lados: LockFileEx não sincroniza acessos mapeados sozinho.
// O byte 0 protege cabeçalho + pixels; o byte 1 reserva o único produtor.
class FrameLock {
public:
    FrameLock(HANDLE file, bool exclusive) : _file(file) {
        _locked = LockFileEx(file, exclusive ? LOCKFILE_EXCLUSIVE_LOCK : 0, 0, 1, 0, &_overlapped) != FALSE;
    }
    ~FrameLock() { if (_locked) UnlockFileEx(_file, 0, 1, 0, &_overlapped); }
    bool ok() const { return _locked; }
private:
    HANDLE _file;
    OVERLAPPED _overlapped{};
    bool _locked;
};
