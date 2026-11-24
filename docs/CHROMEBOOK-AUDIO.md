# Chromebook Linux container audio

## 🇵🇱 Polski

Crostini zwykle udostępnia dźwięk przez most PulseAudio, nie bezpośredni sprzęt ALSA. CPAL korzysta z ALSA, a plugin pulse łączy go z mostem. PipeWire może udostępnić analogiczny PCM pipewire.

Po błędzie MASTER program próbuje rozpoznane pulse, pipewire i default z buforem sterownika; nie wybiera automatycznie przypadkowego hw ani null. Urządzenia są rozpoznawane po identyfikatorze PCM, a nie opisowej nazwie. Zapisane opisy są nadal akceptowane. Próbowane są także kolejne obsługiwane częstotliwości. Mikrofon ponawia mosty wejściowe po błędzie startu.

Zainstaluj libasound2-plugins w kontenerze Debian i sprawdź pactl info (pulseaudio-utils). Nie zastępuj konfiguracji ChromeOS i nie uruchamiaj drugiego serwera PulseAudio. Wybierz System/ALSA i pulse, zacznij bez PFL z buforem automatycznym. Brak mostu/pluginu lub odmowa dostępu do socketu wymaga naprawy kontenera; mikrofon wymaga również uprawnień ChromeOS.

AppImage potrzebuje pluginu ALSA na hoście; Flatpak otrzymuje socket PulseAudio i wymaga obsługi mostu w runtime. Diagnostyka zachowuje błędy. Test symuluje brak sprzętu default i działające pulse z innym opisem; używa null jako kontrolowanego urządzenia testowego, nie potwierdza słyszalnego dźwięku na Chromebooku.

## 🇬🇧 English

Crostini normally exposes audio through a PulseAudio bridge rather than direct ALSA hardware devices. Prospekt DJ uses CPAL's ALSA backend; the ALSA `pulse` plugin connects it to that bridge. PipeWire systems can expose the analogous `pipewire` PCM.

On failure to open MASTER, Linux builds retry enumerated `pulse`, `pipewire`, then `default` PCM devices with the driver-default buffer. They do not automatically choose unrelated `hw:*` outputs or a silent `null` PCM. Recovery is included in audio diagnostics. The Debian package requires libasound2-plugins.

For a Chromebook Debian container, verify `libasound2-plugins` is installed. `pactl info` (from pulseaudio-utils) can check whether the existing ChromeOS audio bridge is reachable. Do not replace ChromeOS audio configuration or start a second PulseAudio server just to run the program. In Audio settings choose system/ALSA and MASTER `pulse` if listed; start with PFL disabled and automatic buffer size. A missing bridge, denied socket or missing plugin requires repair in the container, not ASIO.

AppImage still needs the host/container PulseAudio ALSA plugin. Flatpak already requests the PulseAudio socket; its runtime must provide the ALSA bridge. Device/driver errors are preserved in audio diagnostics for troubleshooting.

Automated Linux checks can simulate a failing ALSA default and an available bridge PCM. Such a test validates fallback and callback operation, not sound on physical Chromebook hardware. Hardware validation requires the exact Chromebook/container error report.

## Linux compatibility fixes

ALSA devices are identified by their actual PCM driver name (`pulse`, `pipewire`, `default`, `hw:…`) rather than the first line of the translated human-readable description. Old saved descriptions remain accepted. This is essential when the PulseAudio PCM is described as “PulseAudio Sound Server” or another friendly label. The same identification applies to microphone inputs and headphone selection.

If ALSA has no default device, automatic output selection is restricted to known audio bridges; it never falls back to the first enumerated device, which can be `null`. Output negotiation tries distinct supported 48 kHz and 44.1 kHz rates and range endpoints, with validated channel/rate values. A failed Linux output retries recognised bridges using driver buffers. Microphone startup also retries recognised input bridges when the initial device fails, while respecting ChromeOS microphone permissions.

Regression checks include a simulated unavailable default hardware card and a working PCM named `pulse` with a different descriptive label. The test uses ALSA's null plugin to verify fallback and rendering, not a real ChromeOS sound server or audible output.
