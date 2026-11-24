# Dysk Google w bibliotece

## 🇵🇱 Polski

Sekcja **CHMURA → DYSK GOOGLE** znajduje się w lewym menadżerze. Kliknięcie otwiera okno logowania i przeglądania plików.

## Pierwsze połączenie

1. W Google Cloud utwórz projekt i włącz Google Drive API.
2. Skonfiguruj ekran zgody OAuth. W fazie testowej dodaj swoje konto jako użytkownika testowego.
3. Utwórz klienta OAuth typu **Aplikacja komputerowa / Desktop app** i pobierz jego JSON.
4. W programie wybierz **Wczytaj konfigurację OAuth…**, a następnie **Połącz z Google**. Dokończ autoryzację w przeglądarce.

Program używa zakresu `drive.readonly`: przegląda pliki i pobiera muzykę; nie zmienia i nie usuwa plików w chmurze. Logowanie stosuje losowy `state`, PKCE S256 i tymczasowy callback na 127.0.0.1. Tokeny oraz konfiguracja klienta pozostają tylko w pamięci bieżącej sesji; nie są zapisywane w settings.json. Po ponownym uruchomieniu należy wczytać konfigurację i zalogować się ponownie. Publiczna dystrybucja konfiguracji OAuth może wymagać weryfikacji aplikacji przez Google.

## Pliki i pobieranie

Okno pokazuje foldery i pliki audio w Moim dysku, po 100 rekordów Google API na stronę (pliki niebędące audio są pomijane). Nawiguj folderami i przyciskami ścieżki; **Następna strona** wczytuje następną stronę. Lewy panel pokazuje skróconą listę aktualnego folderu. Skróty Google Drive, dyski współdzielone i wyszukiwanie całego konta nie są jeszcze obsługiwane.

- **Pobierz** zapisuje plik i dodaje go do lokalnej biblioteki.
- **A/B/C/D** najpierw pobiera utwór, następnie ładuje go na wskazany dek.
- **Pobrane pliki w bibliotece** przełącza bibliotekę na lokalny folder pobrań.

W wersji przenośnej pliki trafiają do `CLOUD/GoogleDrive/<id-pliku>/gdrive-<nazwa>` obok EXE. Różne pliki o tej samej nazwie mają osobne katalogi. `download.json` przechowuje identyfikator, rozmiar i datę modyfikacji wersji pobranej. Zgodna wersja lokalna jest używana bez ponownego pobierania. Istniejąca różniąca się wersja nie jest nadpisywana; program prosi o jej przeniesienie. Pobieranie używa pliku tymczasowego i sprawdza rozmiar, jeśli Google go udostępnia. Rozłączenie nie usuwa lokalnej muzyki. Dla pakietów systemowych Linux obowiązuje wspólny zapisywalny katalog danych opisany w PORTABLE-DATA.md.

Automatycznie sprawdzono walidację konfiguracji OAuth Desktop, PKCE według RFC 7636 i bezpieczne lokalne ścieżki. Rzeczywiste logowanie oraz pobranie pliku z konta użytkownika wymagają własnego klienta OAuth i nie zostały potwierdzone w środowisku deweloperskim.

Ikona Google Drive: oficjalny zasób Google, https://ssl.gstatic.com/images/branding/product/1x/drive_2020q4_48dp.png. Znak Google Drive należy do Google; nie jest objęty licencją AGPL kodu programu. Używany do identyfikacji integracji, bez sugerowania afiliacji lub zatwierdzenia przez Google.


## 🇬🇧 English

CHMURA → DYSK GOOGLE opens connection and file browsing. Create your own Google Cloud project with Drive API enabled, configure OAuth consent/test users and download a Desktop application client JSON. Import it, choose Connect and finish browser authorization. Read-only drive.readonly scope never modifies/deletes cloud files. Random state, PKCE S256 and a temporary 127.0.0.1 callback protect login; client data/tokens stay in session memory. Reimport/login after restart; public OAuth configuration may require Google verification.

Browse My Drive folders and audio, 100 API records per page with nonaudio filtered. Shared drives, shortcuts and whole-account search are unavailable. Download adds local audio; A–D downloads then loads; Downloaded files switches the library to local storage. Portable files live in CLOUD/GoogleDrive/<file-id>/gdrive-<name>, with download.json version metadata. Matching files are reused, differing files are never overwritten. Temporary downloads validate size when available. Disconnect does not delete music; system Linux packages follow PORTABLE-DATA rules. Configuration, PKCE and safe-path tests passed; real account authorization requires your own client and remains hardware/account validation. The official Google Drive icon identifies the integration without endorsement; Google retains trademark rights.
