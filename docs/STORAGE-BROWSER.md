# Storage browser

## 🇵🇱 Polski

FOLDERY udostępniają PULPIT, MOJE DOKUMENTY, DYSKI TWARDE i URZĄDZENIA ZEWNĘTRZNE. Znane foldery użytkownika pochodzą z systemu z awaryjnymi ścieżkami domowymi. Kolory i ulubione foldery działają również tutaj.

Wątek co około 3 s sprawdza zamontowane woluminy. Obecne przy starcie nie generują powiadomień. Nowy otwiera PODŁĄCZONO NOŚNIK Z DANYMI; osobny wątek liczy MP3/WAV/FLAC/AAC/OGG bez rozróżniania wielkości liter. Nie importuje ani nie modyfikuje plików, nie śledzi symlinków i nie przechodzi do innych systemów plików. Błędy odczytu oznaczają wynik częściowy.

Linux czyta producenta/model z sysfs i rozpoznaje USB; Windows korzysta z poleceń Storage PowerShell i API dysków z cache. Brak danych jest jawny. Urządzenie musi być zamontowane i czytelne; sam sprzęt bez montowania nie wystarcza. Liczby dotyczą rozszerzeń, nie zweryfikowanych nagrań. Hot-plug i metadane Windows wymagają sprzętu; testy obejmują rekursję, rozszerzenia, pomijane katalogi i dowiązania.

## 🇬🇧 English

The library's FOLDERY section provides PULPIT (Desktop), MOJE DOKUMENTY (Documents), DYSKI TWARDE (fixed volumes), and URZĄDZENIA ZEWNĘTRZNE (external volumes). Desktop and Documents use the operating system's known user directories, with conventional home-directory fallbacks. Existing imported folders remain available. Folder colors and favorite-folder actions also work on these paths.

A background worker refreshes mounted volumes approximately every three seconds. Volumes already mounted at launch appear without connection alerts. A newly mounted volume opens PODŁĄCZONO NOŚNIK Z DANYMI; inventory continues in a separate worker and updates the notification with individual MP3, WAV, FLAC, AAC and OGG counts and their total. Extension matching is case insensitive. This inventory reads directory entries only, does not import or modify files, does not follow symlinks or cross into other mounted filesystems. Read errors produce a partial-result warning.

Linux obtains device model/vendor information from sysfs and identifies USB devices in addition to removable media. Windows uses Storage PowerShell commands for model/manufacturer/bus information, cached by volume path and device name; removable media is also identified by the platform disk API. Missing metadata is explicitly labeled as unavailable. The device must be mounted and readable by the operating system; attaching unmounted hardware alone does not trigger a scan. Counts identify filename extensions, not decoded/validated audio contents.

Hardware hot-plug and Windows metadata need validation with physical devices; automated tests cover recursive inventory, supported extensions, ignored directories, symlink loops and inaccessible/missing roots.
