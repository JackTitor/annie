
#define WIN32_LEAN_AND_MEAN

#include <audioclient.h>
#include <mmdeviceapi.h>
#include <audiopolicy.h>

#include "mute_control.hpp"

#define ASSERT_HR(hr) \
    if (FAILED(hr)) { \
        CoUninitialize(); \
        return FALSE; \
    }

BOOL SetProcessMute(DWORD dwPID, BOOL bMute)
{
    CoInitializeEx(NULL, 0);
    HRESULT hr = S_OK;

    // Get the audio endpoint for the process
    IMMDeviceEnumerator* pDeviceEnumerator = NULL;
    hr = CoCreateInstance(__uuidof(MMDeviceEnumerator), NULL, CLSCTX_ALL, __uuidof(IMMDeviceEnumerator), (void**)& pDeviceEnumerator);
    ASSERT_HR(hr);
    IMMDevice* pDevice = NULL;
    hr = pDeviceEnumerator->GetDefaultAudioEndpoint(eRender, eMultimedia, &pDevice);
    pDeviceEnumerator->Release();
    ASSERT_HR(hr);

    // Get the audio session manager for the endpoint
    IAudioSessionManager2* pManager = NULL;
    hr = pDevice->Activate(__uuidof(IAudioSessionManager2), CLSCTX_ALL, NULL, (void**)(&pManager));
    pDevice->Release();
    ASSERT_HR(hr);

    // Get the audio session enumerator for the audio session manager
    IAudioSessionEnumerator* pSessionEnumerator = NULL;
    hr = pManager->GetSessionEnumerator(&pSessionEnumerator);
    pManager->Release();
    ASSERT_HR(hr);

    // Enumerate the audio sessions and find the one that corresponds to the process
    IAudioSessionControl2 *pTargetSession = NULL;
    int cSessions;
    hr = pSessionEnumerator->GetCount(&cSessions);
    if (FAILED(hr)) {
        pSessionEnumerator->Release();
        return FALSE;
    }
    for (int i = 0; i < cSessions; i++) {
        IAudioSessionControl *pControl = NULL;
        hr = pSessionEnumerator->GetSession(i, &pControl);
        if (FAILED(hr)) {
            continue;
        }
        IAudioSessionControl2 *pCurrentSession = NULL;
        pControl->QueryInterface(__uuidof(IAudioSessionControl2), (void**)&pCurrentSession);
        DWORD dwCurrentPID;
        pCurrentSession->GetProcessId(&dwCurrentPID);
        if (dwCurrentPID == dwPID) {
            pTargetSession = pCurrentSession;
            break;
        }
        pCurrentSession->Release();
        pControl->Release();
    }
    pSessionEnumerator->Release();

    // Mute the audio session
    if (pTargetSession == NULL) {
        return FALSE;
    }
    ISimpleAudioVolume *pVolume = NULL;
    hr = pTargetSession->QueryInterface(__uuidof(ISimpleAudioVolume), (void**)&pVolume);
    pTargetSession->Release();
    ASSERT_HR(hr);
    pVolume->SetMute(bMute, NULL);
    pVolume->Release();
    ASSERT_HR(hr);

    return TRUE;
}
