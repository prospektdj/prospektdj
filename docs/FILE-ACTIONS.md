# Operacje na plikach biblioteki

## 🇵🇱 Polski

Prawy przycisk na utworze lub okładce → **Plik**:

- **Zmień nazwę pliku…** zmienia nazwę fizycznego pliku, zachowując rozszerzenie. Program aktualizuje kolekcję, ulubione i BANGER BOX. Komentarze i CUE są powiązane z identyfikatorem audio, który się nie zmienia. Najpierw należy wysunąć plik z deków i zakończyć odsłuch, AutoMix oraz indeksowanie.
- **Właściwości systemowe…** uruchamia właściwości Eksploratora Windows, Findera macOS lub dostępnego menadżera Linux (Nautilus, Nemo, Caja, Thunar). Brak obsługi w systemie jest zgłaszany w pasku komunikatów.

Istniejący plik docelowy nie jest nadpisywany. Nazwy z separatorami katalogów, zmienionym rozszerzeniem i nazwami urządzeń Windows są odrzucane. Na FAT/exFAT zmiana nazwy korzysta z kopii z wyłącznym utworzeniem pliku docelowego i usunięciem źródła dopiero po zakończeniu zapisu. Duże pliki na takich nośnikach mogą wymagać dodatkowego miejsca i czasu. Historyczne wpisy sesji zachowują pierwotną ścieżkę.

Sprawdzono test zmiany nazwy, zachowania bajtów i odmowy nadpisania w Linux. Otwarcie natywnych właściwości Windows wymaga sprawdzenia na Windows.

## Menu folderów i rozszerzone operacje

Menu prawego przycisku w lewym drzewie folderów oraz menu **Plik** utworu oferują **Kopiuj**, **Wklej**, **Przenieś do…**, **Zmień nazwę…**, **Utwórz podfolder…** i **Właściwości systemowe…**. Schowek jest wewnętrzny dla programu. Wklejanie na utworze oznacza jego folder nadrzędny. Foldery kopiowane są z zawartością, bez podążania za dowiązaniami symbolicznymi. Operacje nie scalają folderów i nie nadpisują istniejących celów. Przenoszenie wykonuje kopię przed usunięciem źródła i wymaga odpowiedniej ilości miejsca. Nie należy równocześnie modyfikować tych plików w innych aplikacjach.

Przeniesienie aktualizuje ścieżki utworów, BANGER BOX, ulubione foldery i ich kolory. Operacje na folderze zawierającym załadowany utwór są blokowane. Folder programu, jego katalogi nadrzędne oraz stały SAMPLER nie mogą być przemianowane ani przeniesione z tego menu. Dotychczasowe opcje ulubionych folderów, kolorów, importu i odświeżania pozostają dostępne.


## 🇬🇧 English

Right-click tracks/artwork → File, or folders in the left tree, for Copy, Paste, Move to, Rename, Create subfolder and System properties. The clipboard is internal; pasting onto a track uses its parent folder. Rename preserves extensions and updates collection, favorites and BANGER BOX; comments/cues retain audio identity. Eject loaded files and stop preview, AutoMix and indexing first. Windows Explorer, macOS Finder or an available Linux file manager supplies system properties.

Targets are never overwritten or merged. Unsafe filenames, Windows reserved names and extension changes are rejected. FAT/exFAT rename copies to an exclusively created file before source deletion; recursive moves require enough space and do not follow symbolic links. Do not modify the same files concurrently in another app. Moves update folder favorites/colors and track references; historical session paths remain unchanged. The app root, its ancestors and fixed SAMPLER folder cannot be moved/renamed. Automated byte-preservation/no-overwrite tests run on Linux; native Windows properties require Windows testing.
