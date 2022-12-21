
#define WIN32_LEAN_AND_MEAN

#include <audioclient.h>
#include <mmdeviceapi.h>
#include <audiopolicy.h>

#include "mute_control.hpp"

#define SAFE_RELEASE(obj) \
    if(obj != NULL) { \
        obj->Release(); \
        obj = NULL; \
    }

#define CHECK_HR(hr) \
    if (FAILED(hr)) { \
        goto cleanup; \
    }

HRESULT GetVolumeObject(DWORD pid, ISimpleAudioVolume** pVolume)
{
    HRESULT hr = S_OK;
    IMMDeviceEnumerator* enumerator = NULL;
    IMMDevice* speakers = NULL;
    IAudioSessionManager2* manager = NULL;
    IID iid;
    IAudioSessionEnumerator* sessionEnumerator = NULL;
    int sessionCount;
    ISimpleAudioVolume* volume = NULL;

    hr = CoCreateInstance(__uuidof(MMDeviceEnumerator), NULL, CLSCTX_ALL, __uuidof(IMMDeviceEnumerator), (void**)& enumerator);
    CHECK_HR(hr);

    hr = enumerator->GetDefaultAudioEndpoint(eRender, eMultimedia, &speakers);
    CHECK_HR(hr);

    iid = __uuidof(IAudioSessionManager2);
    hr = speakers->Activate(iid, 0, NULL, (void**)(&manager));
    CHECK_HR(hr);

    hr = manager->GetSessionEnumerator(&sessionEnumerator);
    CHECK_HR(hr);

    hr = sessionEnumerator->GetCount(&sessionCount);
    CHECK_HR(hr);

    for (int i = 0; i < sessionCount; i++)
    {
        IAudioSessionControl* controlSimple;
        hr = sessionEnumerator->GetSession(i, &controlSimple);
        CHECK_HR(hr);

        IAudioSessionControl2* control = NULL;
        hr = controlSimple->QueryInterface(__uuidof(IAudioSessionControl2), (void**)& control);
        CHECK_HR(hr);

        DWORD cpid;
        hr = control->GetProcessId(&cpid);
        CHECK_HR(hr);

        if (cpid == pid)
        {
            hr = control->QueryInterface(__uuidof(ISimpleAudioVolume), (void**)& volume);
            CHECK_HR(hr);
            break;
        }

        SAFE_RELEASE(control);
    }

    if (volume == NULL)
    {
        // what else should we return?
        hr = E_FAIL;
        CHECK_HR(hr);
    }

    *pVolume = volume;

    cleanup:
    SAFE_RELEASE(manager);
    SAFE_RELEASE(speakers);
    SAFE_RELEASE(enumerator);
    return hr;
}

HRESULT SetApplicationMute(DWORD pid, BOOL mute)
{
    HRESULT hr = S_OK;
    CoInitialize(0);

    ISimpleAudioVolume* volume = NULL;
    hr = GetVolumeObject(pid, &volume);
    CHECK_HR(hr);

    hr = volume->SetMute(mute, NULL);
    CHECK_HR(hr);

    cleanup:
    SAFE_RELEASE(volume);
    CoUninitialize();
    return hr;
}
