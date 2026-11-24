# Broadcast

## 🇵🇱 Polski

BCAST otwiera ustawienia transmisji. Wysyłany jest ograniczony stereo MASTER, także mikrofon, niezależnie od nagrania i PFL. Transmisję uruchamia użytkownik.

Zainstaluj FFmpeg z libmp3lame (na Windows można umieścić ffmpeg.exe obok programu). Wybierz bitrate MP3, serwer, port źródłowy, hasło i nazwę stacji. Icecast obsługuje login źródła, mount i opcjonalne TLS. Shoutcast wymaga zgodnego wejścia źródłowego v1; negocjacja stream-ID v2 i TLS Shoutcast nie są obsługiwane. Port źródła może różnić się od portu słuchaczy.

Hasła pozostają w pamięci sesji, poza settings.json. Uprawnione procesy systemowe mogą odczytać argumenty FFmpeg zawierające dane Icecast. Surowe błędy serwera nie trafiają do logów aplikacji. Bez TLS dźwięk i dane uwierzytelnienia są przesyłane jawnie.

Ograniczona nieblokująca kolejka chroni odtwarzanie. Przeciążenie sieci zatrzymuje transmisję, a zamknięcie programu lub zmiana audio kończy encoder. Brak automatycznego ponawiania połączenia; stan encodera nie potwierdza odbioru przez słuchacza. Testy lokalnego PUT Icecast i handshake Shoutcast z prawdziwym MP3 włącza PROSPEKT_TEST_FFMPEG=1; publiczne serwery i Windows wymagają osobnego testu.

## 🇬🇧 English

BCAST in the main header opens Settings → Broadcast. Live streaming captures the limited stereo MASTER output, including an enabled microphone, independently of recording and headphone cue monitoring. Transmission never starts automatically.

Install FFmpeg with the libmp3lame encoder (or put ffmpeg.exe beside the Windows executable). Choose MP3 bitrate, server hostname/IP, source port, source password and station name. Icecast additionally accepts a source username and mount path; TLS is optional when supported by the server. Shoutcast uses its legacy v1 source password handshake: use the server's configured source port, which may differ from the listener port. Native Shoutcast v2 stream-ID negotiation and TLS are not implemented; a compatible v1 source input is required.

Passwords are excluded from settings serialization and retained only in session memory. Icecast credentials are passed to FFmpeg and can be visible to processes permitted to inspect command arguments. Raw server/FFmpeg error output is not displayed, avoiding credentials in application logs. Without TLS the source credentials and audio travel unencrypted.

Audio blocks use a bounded, nonblocking queue; network congestion stops the stream rather than blocking audio playback. Stop, app closure, and audio-device changes terminate the encoder. There is no automatic reconnect. The status reports encoder operation rather than claiming that a listener has received audio. Configure and monitor the destination server separately.

Local integration tests exercise authenticated Icecast PUT and the Shoutcast password/header handshake and verify receipt of encoded MP3 bytes using real FFmpeg. Set PROSPEKT_TEST_FFMPEG=1 to enable these tests. Public servers, TLS certificates and physical Windows installations require separate validation.
