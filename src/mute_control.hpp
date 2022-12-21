#pragma once

typedef unsigned long DWORD;
typedef long HRESULT;
typedef int BOOL;

extern "C" HRESULT SetApplicationMute(DWORD pid, BOOL mute);
