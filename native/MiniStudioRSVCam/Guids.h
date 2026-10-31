// Identidade COM da câmera virtual "Mini Studio RS Cam".
//
// CLSID_MiniStudioCamSource é a coclass da media source registrada em
// HKLM\Software\Classes\CLSID\{...}\InprocServer32. O Frame Server do Windows a instancia
// (CoCreateInstance) quando um app abre a câmera; é este mesmo CLSID que passamos como
// sourceId em MFCreateVirtualCamera. NÃO reutilizar este GUID em outro projeto.
#pragma once

#include <initguid.h>
#include <guiddef.h>

// {F0C418C8-9211-479E-BAAE-3CBF90F15D37}
DEFINE_GUID(CLSID_MiniStudioCamSource,
    0xf0c418c8, 0x9211, 0x479e, 0xba, 0xae, 0x3c, 0xbf, 0x90, 0xf1, 0x5d, 0x37);

#define MSVCAM_CLSID_STRING   L"{F0C418C8-9211-479E-BAAE-3CBF90F15D37}"
#define MSVCAM_FRIENDLY_NAME  L"Mini Studio RS Cam"
