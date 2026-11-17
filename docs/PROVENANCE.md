# Pochodzenie

Specyfikacją funkcjonalną był plik Prospekt_DJ_Controller.html dostarczony przez użytkownika (SHA-256: `480e15713d7f8e23c95974d62e74e31dd937246b84dd9a10d4cdd965e7ab72d8`).

Program Rust napisano od nowa: interfejs, sterowanie, DSP, pętle, sampler, MIDI, kolejkę i zapis plików. Nie dołączono żadnego JavaScriptu, CSS ani HTML z poprzedniej wersji. Algorytmy zachowują założenia funkcjonalne prototypu, m.in. analizę obwiedni dla BPM i interpolację PCM dla jogu; nowy limiter i filtracja nie gwarantują bitowej identyczności dźwięku z Web Audio.

Pierwotną wersję 2.0 napisano bez kopiowania kodu, map MIDI lub grafik OpenFadeR. Biblioteki Rust pochodzą z crates.io; ich wersje i licencje są zapisane w Cargo.lock i docs/DEPENDENCIES.md. Ikona jest własnym zasobem przygotowanym w poprzedniej części zadania.

W wersji 2.2 użytkownik dostarczył paczkę mapowań. Przekonwertowano deklaracje MIDI i zachowano metadane autorów; nie dołączono skryptów ani silnika programu źródłowego. Pochodzenie, SHA-256 i licencje danych opisano w licenses/controller-mappings/README.md. Wydanie 2.2 BETA z tym katalogiem jest AGPL-3.0-or-later.

Original grapefruit-pink PROSPEKT DJ logo and matching vinyl icon were created for this BETA application. The main header uses the graphic logo by default; a text/font fallback remains available in settings.
