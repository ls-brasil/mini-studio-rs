#include "Common.h"
#include "MediaSource.h"
#include "Guids.h"   // por último: initguid.h define CLSID_MiniStudioCamSource nesta TU só

#include <string>
#include "FrameWriter.h"

std::atomic<long> g_objectCount{0};
static HMODULE g_module = nullptr;
static ComPtr<IMFVirtualCamera> g_sessionVcam;
static bool g_mfStarted = false;
static FrameWriter g_writer;
extern "C" HRESULT MSVCam_StopSession();

// ------------------------------------------------------------ Activator (IMFActivate)
// O que o Frame Server instancia via CoCreateInstance(CLSID). ActivateObject devolve a
// MediaSource. Carrega os atributos que identificam a câmera virtual.

class Activator : public RuntimeClass<
    RuntimeClassFlags<ClassicCom>,
    ChainInterfaces<IMFActivate, IMFAttributes>>
{
public:
    Activator() { g_objectCount.fetch_add(1); }
    ~Activator() { g_objectCount.fetch_sub(1); }

    HRESULT Initialize()
    {
        HRESULT hr = MFCreateAttributes(&_attributes, 4);
        if (FAILED(hr)) return hr;

        _attributes->SetUINT32(MF_VIRTUALCAMERA_PROVIDE_ASSOCIATED_CAMERA_SOURCES, 1);
        _attributes->SetGUID(MFT_TRANSFORM_CLSID_Attribute, CLSID_MiniStudioCamSource);

        _source = Make<MediaSource>();
        if (!_source) return E_OUTOFMEMORY;
        return _source->Initialize(_attributes.Get());
    }

    STDMETHODIMP ActivateObject(REFIID riid, void** ppv) override
    {
        if (!ppv) return E_POINTER;
        if (!_source) return E_UNEXPECTED;
        return _source->QueryInterface(riid, ppv);
    }

    STDMETHODIMP ShutdownObject() override
    {
        if (_source) _source->Shutdown();
        return S_OK;
    }

    STDMETHODIMP DetachObject() override
    {
        _source.Reset();
        return S_OK;
    }

    FORWARD_IMFATTRIBUTES(_attributes)

private:
    ComPtr<IMFAttributes> _attributes;
    ComPtr<MediaSource> _source;
};

// ------------------------------------------------------------ ClassFactory

class ClassFactory : public RuntimeClass<RuntimeClassFlags<ClassicCom>, IClassFactory>
{
public:
    STDMETHODIMP CreateInstance(IUnknown* outer, REFIID riid, void** ppv) override
    {
        if (!ppv) return E_POINTER;
        *ppv = nullptr;
        if (outer) return CLASS_E_NOAGGREGATION;

        auto activator = Make<Activator>();
        if (!activator) return E_OUTOFMEMORY;
        HRESULT hr = activator->Initialize();
        if (FAILED(hr)) return hr;
        return activator->QueryInterface(riid, ppv);
    }

    ClassFactory() { g_objectCount.fetch_add(1); }
    ~ClassFactory() { g_objectCount.fetch_sub(1); }
    STDMETHODIMP LockServer(BOOL lock) override { g_objectCount.fetch_add(lock ? 1 : -1); return S_OK; }
};

// ------------------------------------------------------------ registro COM (HKLM)

static HRESULT RegisterComServer()
{
    wchar_t dllPath[MAX_PATH];
    if (!GetModuleFileNameW(g_module, dllPath, MAX_PATH))
        return HRESULT_FROM_WIN32(GetLastError());

    wchar_t clsid[64];
    StringFromGUID2(CLSID_MiniStudioCamSource, clsid, 64);

    std::wstring key = L"Software\\Classes\\CLSID\\";
    key += clsid;
    key += L"\\InprocServer32";

    HKEY h = nullptr;
    LSTATUS s = RegCreateKeyExW(HKEY_LOCAL_MACHINE, key.c_str(), 0, nullptr,
                                REG_OPTION_NON_VOLATILE, KEY_WRITE, nullptr, &h, nullptr);
    if (s != ERROR_SUCCESS)
        return HRESULT_FROM_WIN32(s);

    s = RegSetValueExW(h, nullptr, 0, REG_SZ, (const BYTE*)dllPath,
                   (DWORD)((wcslen(dllPath) + 1) * sizeof(wchar_t)));
    static const wchar_t both[] = L"Both";
    if (s == ERROR_SUCCESS) s = RegSetValueExW(h, L"ThreadingModel", 0, REG_SZ, (const BYTE*)both, sizeof(both));
    RegCloseKey(h);
    return HRESULT_FROM_WIN32(s);
}

static HRESULT UnregisterComServer()
{
    wchar_t clsid[64];
    StringFromGUID2(CLSID_MiniStudioCamSource, clsid, 64);
    std::wstring key = L"Software\\Classes\\CLSID\\";
    key += clsid;
    const LSTATUS s = RegDeleteTreeW(HKEY_LOCAL_MACHINE, key.c_str());
    return s == ERROR_FILE_NOT_FOUND ? S_OK : HRESULT_FROM_WIN32(s);
}

// ------------------------------------------------------------ exports COM padrão

STDAPI DllGetClassObject(REFCLSID rclsid, REFIID riid, void** ppv)
{
    if (!ppv) return E_POINTER;
    *ppv = nullptr;
    if (rclsid != CLSID_MiniStudioCamSource)
        return CLASS_E_CLASSNOTAVAILABLE;

    auto factory = Make<ClassFactory>();
    if (!factory) return E_OUTOFMEMORY;
    return factory->QueryInterface(riid, ppv);
}

STDAPI DllCanUnloadNow()
{
    return g_objectCount.load() == 0 ? S_OK : S_FALSE;
}

STDAPI DllRegisterServer()
{
    return RegisterComServer();
}

STDAPI DllUnregisterServer()
{
    return UnregisterComServer();
}

STDAPI DllInstall(BOOL install, LPCWSTR)
{
    return install ? DllRegisterServer() : DllUnregisterServer();
}

// ------------------------------------------------------------ controle de sessão (FFI do app Rust)
// O MiniStudio (não elevado) cria/remove uma câmera virtual de sessão. Ela existe enquanto o
// processo do app viver — some ao fechar. O registro COM em HKLM (feito no install) permite ao
// Frame Server carregar esta mesma DLL quando um app abre a câmera.

extern "C" HRESULT MSVCam_StartSession()
{
    if (g_sessionVcam) return S_OK;
    HRESULT hr = g_writer.Open();
    if (FAILED(hr)) return hr;
    hr = MFStartup(MF_VERSION, MFSTARTUP_LITE);
    if (FAILED(hr)) { g_writer.Close(); return hr; }
    g_mfStarted = true;
    hr = MFCreateVirtualCamera(MFVirtualCameraType_SoftwareCameraSource,
        MFVirtualCameraLifetime_Session, MFVirtualCameraAccess_CurrentUser,
        MSVCAM_FRIENDLY_NAME, MSVCAM_CLSID_STRING, nullptr, 0, &g_sessionVcam);
    if (SUCCEEDED(hr)) hr = g_sessionVcam->Start(nullptr);
    if (FAILED(hr)) MSVCam_StopSession();
    return hr;
}

extern "C" HRESULT MSVCam_Publish(const uint8_t* pixels, uint32_t bytes)
{
    return g_writer.Publish(pixels, bytes);
}

extern "C" HRESULT MSVCam_StopSession()
{
    HRESULT hr = S_OK;
    g_writer.Close();
    if (g_sessionVcam) {
        hr = g_sessionVcam->Stop();
        HRESULT removed = g_sessionVcam->Remove();
        if (SUCCEEDED(hr)) hr = removed;
        g_sessionVcam->Shutdown();
        g_sessionVcam.Reset();
    }
    if (g_mfStarted) { MFShutdown(); g_mfStarted = false; }
    return hr;
}

// ------------------------------------------------------------ DllMain

BOOL WINAPI DllMain(HINSTANCE inst, DWORD reason, LPVOID)
{
    if (reason == DLL_PROCESS_ATTACH)
    {
        g_module = inst;
        DisableThreadLibraryCalls(inst);
    }
    return TRUE;
}
