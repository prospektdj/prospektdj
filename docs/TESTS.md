# Weryfikacja / Validation

## 🇵🇱 Polski

Testy obejmują silnik audio, pętle i CUE, efekty, nagrywanie, odsłuch, mikrofon, mapowania MIDI, rekomendacje i ich historię, sesje grania, przenośne dane, operacje na plikach, OAuth/PKCE i inwentaryzację nośników. Konfiguracja CI oraz wyniki kompilacji są w GitHub Actions. Polecenia opisano w BUILD.md.

Testy logiczne oraz symulowane ALSA nie potwierdzają działania wszystkich sterowników. Na Windows wymagane są próby ASIO4ALL/Realtek i fizycznych interfejsów. Chromebook wymaga sprawdzenia mostu audio, mikrofonu i wydajności na konkretnym modelu. Dwa kontrolery, USB hot-plug i rzeczywisty Google OAuth wymagają osobnych testów. Nie przedstawiamy prób Windows 7 jako potwierdzonej zgodności.

## 🇬🇧 English

Tests cover the audio engine, loops/cues, effects, recording, preview, microphone, MIDI mapping, recommendations/logging, playback history, portable data, file operations, OAuth/PKCE and storage inventory. CI configuration and build results are available in GitHub Actions; commands are documented in BUILD.md.

Logic tests and simulated ALSA cannot establish compatibility with every driver. Windows requires ASIO4ALL/Realtek and physical interface testing. Chromebook needs bridge, microphone and performance testing on the actual model. Dual controllers, USB hot-plug and real Google OAuth require separate validation. Windows 7 attempts are not represented as verified compatibility.
