# ASIO4ALL (Windows)

## 🇵🇱 Polski

ASIO4ALL to opcjonalny sterownik Windows instalowany osobno, także dla Realteka. Program zachowuje WASAPI i sterowniki ASIO producentów.

1. Pobierz sterownik z https://asio4all.org/ i zainstaluj obsługę tej samej architektury co program (32/64-bit).
2. W ustawieniach audio kliknij „Wykryj i wybierz ASIO4ALL”. Zostanie wybrany zarejestrowany sterownik i jego bufor domyślny; audio nie startuje samoczynnie.
3. Zastosuj ustawienia, a następnie otwórz „Panel sterownika ASIO / ASIO4ALL…” i włącz wyjście Realtek WDM.
4. Ustaw prawidłowe pary MASTER/PFL. Jedno wyjście stereo nie zapewni dwóch niezależnych odsłuchów stereo.

Jeżeli inne aplikacje blokują urządzenie WDM, zamknij je lub wybierz inne wyjście. Zmiana bufora/kanałów może wymagać restartu audio. Instalatora nie dołączamy i nie uruchamiamy automatycznie. Działanie sterownika i panelu wymaga testu na Windows; Linux korzysta z własnego backendu.

## 🇬🇧 English

ASIO4ALL is an optional, separately installed Windows ASIO driver, including for Realtek devices. The application retains WASAPI and native interface ASIO drivers.

1. Download ASIO4ALL from https://asio4all.org/ and install support matching the application's architecture (32/64-bit).
2. In audio settings choose **Wykryj i wybierz ASIO4ALL**. This selects an installed registered ASIO4ALL driver and resets the buffer selection to the driver default; it does not start audio automatically.
3. Apply the audio settings. **Panel sterownika ASIO / ASIO4ALL…** opens the active driver's panel; enable the intended Realtek WDM output there.
4. Choose valid MASTER/PFL output channel pairs. A single stereo Realtek output cannot supply independent stereo MASTER and stereo headphones simultaneously.

If another application holds the WDM device, close it or select another available output. Restart application audio after changing the driver's channel configuration or buffer if required. ASIO4ALL is Windows-only; Linux continues to use its system audio backend.

The installer and driver are not bundled or installed automatically. Windows/Realtek hardware validation is required; Linux compilation cannot verify the actual Windows driver or its panel.
