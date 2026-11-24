# Budowanie i platformy / Build and platforms

## 🇵🇱 Polski

Wydanie: pre-alpha 0.0.1 test (`0.0.1-prealpha-test`). Kod wymaga Rust 1.89 lub nowszego. Testy: `cargo test --locked`; kontrola: `cargo clippy --locked --all-targets -- -D warnings`; formatowanie: `cargo fmt --all --check`.

- Windows x64 i x86: targety `x86_64-pc-windows-msvc` i `i686-pc-windows-msvc`, sterownik ASIO tej samej architektury. LLVM/libclang i zweryfikowany SDK są przygotowywane przez CI.
- Linux x64: archiwum, DEB, AppImage i Flatpak. Linux x86: archiwum i DEB, z 32-bitowymi bibliotekami ALSA/X11/OpenGL. Aktualny runtime Flatpak i narzędzia AppImage użyte w projekcie są x64.
- Windows 7: oddzielna eksperymentalna próba targetów `*-win7-windows-msvc`, nightly i `-Z build-std`. Zależności i importy API mogą uniemożliwić budowę lub uruchomienie. Wyniki prób są dołączane do wydania; nie oznaczamy nieudanego pliku jako działającej paczki.

Testy audio kontenera: ustaw `ALSA_CONFIG_PATH` na bezwzględną ścieżkę `tests/fixtures/alsa/broken-default.conf`, `PROSPEKT_TEST_BROKEN_ALSA=1`, a następnie uruchom `cargo test --locked --test audio_backend -- --test-threads=1`. To test symulowanego mostu, nie sprzętu ChromeOS. `PROSPEKT_TEST_FFMPEG=1` włącza lokalne testy transmisji z prawdziwym FFmpeg.

## 🇬🇧 English

Release: pre-alpha 0.0.1 test (`0.0.1-prealpha-test`). Rust 1.89 or newer is required. Tests: `cargo test --locked`; lint: `cargo clippy --locked --all-targets -- -D warnings`; formatting: `cargo fmt --all --check`.

- Windows x64/x86: `x86_64-pc-windows-msvc` and `i686-pc-windows-msvc`, with architecture-matched ASIO drivers. CI prepares LLVM/libclang and the verified SDK.
- Linux x64: archive, DEB, AppImage and Flatpak. Linux x86: archive and DEB with 32-bit ALSA/X11/OpenGL libraries. The current Flatpak runtime and AppImage tooling used by this project are x64.
- Windows 7: separate experimental `*-win7-windows-msvc` attempts using nightly and `-Z build-std`. Dependencies/API imports may prevent building or launching. Attempt results accompany the release; failed outputs are never presented as working packages.

Container audio tests: set `ALSA_CONFIG_PATH` to the absolute `tests/fixtures/alsa/broken-default.conf` path, set `PROSPEKT_TEST_BROKEN_ALSA=1`, then run `cargo test --locked --test audio_backend -- --test-threads=1`. This validates a simulated bridge, not ChromeOS hardware. `PROSPEKT_TEST_FFMPEG=1` enables local integration tests with real FFmpeg.
