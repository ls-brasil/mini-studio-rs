cmake_minimum_required(VERSION 3.20)
project(MiniStudioRSVCam LANGUAGES CXX RC)

file(STRINGS "${CMAKE_CURRENT_SOURCE_DIR}/../../Cargo.toml" MS_VERSION_LINE REGEX "^version =")
string(REGEX MATCH "[0-9]+\\.[0-9]+\\.[0-9]+" MS_VERSION "${MS_VERSION_LINE}")
string(REPLACE "." "," MS_VERSION_COMMAS "${MS_VERSION}")
configure_file(version.rc.in "${CMAKE_CURRENT_BINARY_DIR}/version.rc" @ONLY)

# Media source da câmera virtual "Mini Studio RS Cam".
# DLL COM in-proc carregada pelo Frame Server do Windows (via MFCreateVirtualCamera).
# x64 apenas — o Frame Server (64-bit) hospeda a source e faz a ponte para consumidores
# de qualquer bitness, então não é preciso build 32-bit (ao contrário do DirectShow).

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

add_library(MiniStudioRSVCam SHARED
    "${CMAKE_CURRENT_BINARY_DIR}/version.rc"
    dllmain.cpp
    MediaSource.cpp
    MediaStream.cpp
    FrameReader.cpp
    FrameWriter.cpp
    MiniStudioRSVCam.def
)

# CRT estático (/MT): a DLL é carregada em processos arbitrários (Frame Server, apps
# consumidores) que podem não ter o VC++ redist — igual ao módulo do OBS.
set_property(TARGET MiniStudioRSVCam PROPERTY
    MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>")

target_compile_definitions(MiniStudioRSVCam PRIVATE
    UNICODE _UNICODE WIN32_LEAN_AND_MEAN NOMINMAX)

target_link_libraries(MiniStudioRSVCam PRIVATE
    mfsensorgroup   # MFCreateVirtualCamera / IMFVirtualCamera
    mfplat
    mf
    mfuuid
    mfreadwrite
    shlwapi
    shell32
    ole32
    oleaut32
    ksuser
    uuid
    advapi32
)

set_target_properties(MiniStudioRSVCam PROPERTIES
    OUTPUT_NAME "MiniStudioRSVCam"
    PREFIX ""
)

enable_testing()
add_executable(vcam_transport_test transport_test.cpp FrameReader.cpp FrameWriter.cpp)
target_compile_definitions(vcam_transport_test PRIVATE UNICODE _UNICODE WIN32_LEAN_AND_MEAN NOMINMAX)
target_link_libraries(vcam_transport_test PRIVATE shell32 ole32 uuid)
add_test(NAME vcam_transport COMMAND vcam_transport_test)
set_tests_properties(vcam_transport PROPERTIES TIMEOUT 20)

add_executable(vcam_media_test media_test.cpp MediaSource.cpp MediaStream.cpp FrameReader.cpp FrameWriter.cpp)
target_compile_definitions(vcam_media_test PRIVATE UNICODE _UNICODE WIN32_LEAN_AND_MEAN NOMINMAX)
target_link_libraries(vcam_media_test PRIVATE mfsensorgroup mfplat mf mfuuid mfreadwrite shell32 ole32 oleaut32 ksuser uuid)
add_test(NAME vcam_media COMMAND vcam_media_test)
set_tests_properties(vcam_media PROPERTIES TIMEOUT 20)
