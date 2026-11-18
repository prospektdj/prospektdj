# Prompt: następna aktualizacja Prospekt DJ — 11 punktów

Zaktualizuj istniejący projekt **Prospekt DJ**: https://github.com/prospektdj/prospektdj.

Zrealizuj poniższe 11 punktów jako jedną spójną aktualizację istniejącej aplikacji. Potrzebuję działających funkcji i zmienionego interfejsu. Czytaj wymagania razem z sześcioma załącznikami. **Najważniejszy jest punkt 7: wygląd, rozmieszczenie i działanie głównego miksera muszą odpowiadać referencji.** Nie zastępuj tego dowolnym, ogólnym projektem interfejsu DJ.

To prompt do implementacji kolejnej aktualizacji. Zachowaj już działające funkcje programu, w tym 2/4 decki, audio, odsłuch PFL, MIDI, scratch, pitch bend, osiem Hot Cue, waveformy, efekty, pętle, automix, nagrywanie, skrzynki z emoji, przeglądarkę okładek i ich pobieranie z internetu.

## Materiały i aktualny punkt wyjścia

W tej wiadomości nazwy kilku załączników otrzymały dopiski. Korzystaj z **nowych** załączników według poniższej tabeli, aby nie pomylić ich z obrazami z poprzedniej aktualizacji.

| Nazwa w opisie użytkownika | Faktyczna nazwa załącznika | Co przedstawia i do czego służy |
| --- | --- | --- |
| `1.png` | `1(2).png` | Najważniejszy wzór: kompaktowy mikser, małe knoby, pionowe fadery, mierniki, PFL, filtr i crossfader. |
| `2.png` | `2(1).png` | Prostokątne, zwarte komórki i przyciski w tabeli; wzór dla Manual Loop oraz kompaktowych paneli. |
| `2.2.png` | `2.2.png` | Wzór Auto Loop: wartości długości pętli, zaznaczenie aktywnej wartości i strzałki. |
| `3.png` | `3(1).png` | Rozbudowany TRACK INFO; na górze czerwono zaznaczony miniaturowy waveform z kolorowymi Cue. |
| `4.png` | `4.png` | Kolumna PREVIEW w tabeli utworów, wyróżniona czerwonym prostokątem. |
| `5.png` | `5.PNG` | Czerwony prostokąt wyznacza docelowe miejsce kompaktowego samplera wewnątrz decka, między częścią z padami a obszarem suwaka i jogu. |

Czerwone ramki na obrazach wskazują element lub miejsce do zmiany. Nie są elementami docelowego interfejsu. Osobno traktuj czerwony kolor podany w wymaganiach dla LOW, samplera i REC — ten kolor ma zostać użyty w programie.

Przed implementacją sprawdź aktualne instrukcje i kod repozytorium. Stan sprawdzony przy pisaniu tego promptu, 18 listopada 2026 r.: `main`, rewizja `abf9112ceec9db95ab17e4a7a48cad6e16f25d7c`, wersja `2.3.0-prebeta.1`. Program jest natywną aplikacją Rust z egui/eframe. Przy późniejszym wykonaniu promptu zweryfikuj aktualną rewizję. Zachowaj tę architekturę i istniejące mechanizmy pracy w tle.

## Punkt 1. Trwała baza Hot Cue przypisanych do utworu

Utwór ma pamiętać nadane punkty Hot Cue. Przykład oczekiwanego działania: ładuję MP3, ustawiam Cue 1, 3 i 8, ładuję inny utwór, po czym wracam do pierwszego — jego Cue pojawiają się ponownie. Tak samo ma być po zamknięciu i ponownym uruchomieniu programu oraz po załadowaniu tego utworu do innego decka.

Zaimplementuj trwały magazyn powiązany z tożsamością utworu, najlepiej SQLite. Powiąż go z obecną biblioteką i jej migracjami. Zapisuj co najmniej numer slotu 1–8, pozycję czasową z dokładnością co najmniej do milisekund, informację o pustym slocie oraz wersję zapisu. Kolory korzystają z istniejącej wspólnej palety ośmiu Hot Cue. Jeśli dodajesz nazwy punktów, także je zachowuj.

Wymagania działania:

- Ustawienie, aktualizacja i usunięcie Cue przez mysz, klawiaturę albo MIDI zapisują zmianę automatycznie.
- Zapis działa w tle, transakcyjnie; przy zamykaniu programu oczekujące zmiany są bezpiecznie kończone.
- Punkty są własnością utworu, a nie numeru decka, pozycji na liście czy chwilowego obiektu PCM.
- Przy załadowaniu utworu przywróć punkty przed ich pierwszym użyciem; opóźniony wynik odczytu nie może nadpisać punktów nowszego utworu.
- Wartość `0:00.000` jest poprawnym punktem, a nie oznaczeniem pustego slotu.
- Zmiana tempa nie zmienia zapisanej pozycji w czasie źródłowego nagrania.
- Po usunięciu punkt nie może wrócić po ponownym załadowaniu.
- Jeśli ten sam utwór znajduje się jednocześnie na dwóch deckach, stosuj jeden zapisany bank i deterministycznie synchronizuj zmiany między widokami.
- Zapis nie może wymagać modyfikowania oryginalnego MP3.

Zastosuj trwały identyfikator utworu i mechanizm naprawy ścieżki po przeniesieniu pliku. Sam tytuł i wykonawca nie wystarczają do identyfikacji. Zachowaj dane przy zmianie tagów lub lokalizacji; nie łącz różnych remixów tylko dlatego, że mają podobną nazwę. Obsłuż brakujące pliki i konflikt dopasowania.

Istniejące punkty bieżącej sesji powinny trafić do nowego magazynu, jeśli podczas migracji są dostępne. Zachowaj dotychczasową bibliotekę, skrzynki, ulubione i ustawienia.

## Punkt 2. TRACK INFO, historia, oceny, komentarze i podgląd fali w bibliotece

### 2A. Prawy panel TRACK INFO

Rozbuduj istniejący panel informacji o **zaznaczonym utworze biblioteki**, zgodnie z `3(1).png`. Panel ma działać także wtedy, gdy utwór nie jest obecnie załadowany do decka.

Na samej górze, w miejscu czerwonego wyróżnienia, umieść mały, poziomy waveform całego utworu. Powinien zawierać rzeczywisty przebieg audio oraz kolorowe markery zapisanych Hot Cue odczytanych z bazy z punktu 1. Dalej umieść okładkę i czytelnie uporządkowane informacje.

| Pole | Wymagane zachowanie |
| --- | --- |
| Tytuł, wykonawca, remix/wersja, album, gatunek, rok | Odczyt rzeczywistych metadanych, z czytelną obsługą braku wartości. |
| Czas, BPM, tonacja | Dane zgodne z biblioteką i analizą utworu. |
| Ocena | Klikalne 1–5 gwiazdek; możliwość usunięcia oceny do stanu „bez oceny”. |
| Komentarz | Edycja wielowierszowego tekstu, zapis w bazie, odczyt po restarcie. |
| Pierwsze wykrycie / dodanie do kolekcji | Trwała data pierwszego zarejestrowania utworu, z informacją o źródle przy imporcie. |
| Data utworzenia pliku na dysku | Wyświetl, jeśli system plików ją udostępnia. To osobna informacja od pierwszego wykrycia przez program. |
| Pierwsze znane odtworzenie | Najwcześniejsze wiarygodnie znane odtworzenie z własnej lub zaimportowanej historii. |
| Ostatnie odtworzenie | Ostatnie znane odtworzenie wraz z datą i godziną, jeśli źródło daje taką dokładność. |
| Ostatnie przed bieżącym uruchomieniem | Ostatnie odtworzenie sprzed startu obecnej sesji; wartość nie znika po zagraniu utworu teraz. |
| Liczba odtworzeń | Trwały licznik z rozróżnieniem danych lokalnych i zaimportowanych, gdy jest to potrzebne dla poprawności. |

Wyjaśnienie dat: użytkownik chce wiedzieć, kiedy utwór „pojawił się na dysku”. Program może wiarygodnie pokazać datę utworzenia pliku, jeśli system ją zna, oraz datę pierwszego wykrycia w bibliotece. Nie przedstawiaj daty modyfikacji ani systemowego `ctime` jako pewnej daty pojawienia się pliku. Nieznane wartości pokazuj jako „Brak danych”. Zachowuj informację, czy źródło dostarcza pełny czas, czy wyłącznie datę dzienną.

Za „klasyfikację” w tym wymaganiu przyjmij przede wszystkim ocenę 1–5 gwiazdek. Gwiazdki są niezależne od istniejącego oznaczenia Ulubione. Oceny i komentarze mają być zachowane po odświeżeniu tagów, ponownym indeksowaniu oraz restarcie. Dane użytkownika nie mogą zostać wyzerowane przez ponowny skan pliku.

### 2B. Historia i sposób liczenia odtworzeń

Dodaj rejestrowanie sesji i zdarzeń odtworzeń oraz widoki „Ostatnio odtwarzane” i „Najczęściej odtwarzane”. Samo załadowanie do decka, analiza pliku lub odczyt waveformu nie są odtworzeniem.

Przyjmij jako domyślną zasadę zaliczenie po 30 sekundach faktycznego odtwarzania na torze MASTER; dla utworów krótszych niż minuta można użyć połowy długości. Próg powinien być opisany i konfigurowalny w ustawieniach biblioteki. Sam odsłuch w podglądzie/PFL bez podania na MASTER nie zwiększa domyślnego licznika.

Licz realny czas odtwarzania, a nie różnicę pozycji playheada po przewinięciu. Pauza/wznowienie, kliknięcia Hot Cue, przewijanie, scratch oraz kolejne obroty tej samej pętli nie mogą naliczać wielu odtworzeń jednego wykonania. Zdarzenie otrzymuje własny identyfikator i jest zapisane tylko raz. Historia ma działać także przy automiksie i na deckach C/D. Daty przechowuj w spójnym formacie, a prezentuj w lokalnej strefie czasowej.

„Ostatnie przed bieżącym uruchomieniem” ustal względem początku obecnej sesji. Oddziel je od aktualizowanej wartości „Ostatnie odtworzenie”. Przy niepełnej historii stosuj określenie „pierwsze znane”, zamiast deklarować pewną datę pierwszego odtworzenia w życiu pliku.

### 2C. Import z Serato, VirtualDJ, rekordbox i Mixxx

Program ma **zapytać użytkownika o import istniejącej biblioteki i historii** z tych czterech aplikacji. Dodaj kreator importu dostępny po aktualizacji/pierwszej konfiguracji oraz później z ustawień. Pytanie może brzmieć: „Czy chcesz zaimportować bibliotekę, historię odtworzeń i informacje o utworach z innego programu DJ?”. Pozwól wybrać aplikację, lokalizację źródła i zakres danych. Umożliw pominięcie i opcję „Nie pytaj ponownie”. Nie pokazuj tego dialogu przy każdym uruchomieniu.

Import ma rzeczywiście przenosić dostępne metadane, playlisty/skrzynki, oceny, komentarze, liczniki i historię — odpowiednio do formatu. Jeśli są dostępne Cue, zintegruj je z trwałym magazynem po rozwiązaniu konfliktów. Nie oznaczaj wszystkich formatów jako w pełni obsługiwanych bez sprawdzenia ich zawartości.

| Aplikacja | Źródło do implementacji i sprawdzenia | Ważne rozróżnienie |
| --- | --- | --- |
| Serato | Biblioteka i skrzynki oraz eksport sesji historii, np. CSV; pliki natywne tylko dla faktycznie rozpoznanego i przetestowanego wariantu. | Historia sesji jest osobnym źródłem od kolekcji. Nie zakładaj, że pojedyncza baza lub skrzynka zawiera wszystkie daty, oceny i liczniki. |
| VirtualDJ | Rozpoznana wersja `database.xml` oraz playlisty/pliki historii. | Odczytaj dostępne pola, m.in. first seen, first/last play i play count, jeśli są w danym wariancie. Nie pomyl jednostek czasu i BPM. |
| rekordbox | Eksport XML kolekcji i playlist; dodatkowe dane historii z obsługiwanego eksportu lub czytnika bazy, jeżeli faktycznie zaimplementowany. | XML przewiduje m.in. `DateAdded`, `PlayCount`, `LastPlayed`, `Comments` i `Rating`. Nie odtwarza automatycznie pełnej listy wszystkich sesji. |
| Mixxx | Wersjonowana baza `mixxxdb.sqlite`, powiązania plików, playlist/skrzynki i historia. | Sprawdź schemat i dostępność pól, m.in. `timesplayed`, `last_played_at`, `datetime_added`, ocen, komentarzy i playlist historii. To import biblioteki, nie mapowania kontrolera MIDI. |

Obsłuż następujące zasady:

- Najpierw podgląd: liczba znalezionych i dopasowanych utworów, dostępne pola, brakujące ścieżki, konflikty oraz ograniczenia źródła.
- Źródłowe biblioteki odczytuj bez ich modyfikowania. Dla aktywnej bazy SQLite użyj spójnego odczytu/snapshotu uwzględniającego WAL, zamiast kopiować sam plik i ignorować bieżące zmiany.
- Powtórny import tego samego źródła ma być idempotentny: nie tworzy kolejnych kopii utworów i zdarzeń ani nie zwiększa w kółko play count.
- Nie dodawaj bezwarunkowo do siebie licznika zbiorczego oraz wszystkich zdarzeń historii dotyczących tych samych odtworzeń. Przechowuj pochodzenie, identyfikatory źródeł i zakres importowanej historii.
- Przy kilku źródłach nie udawaj, że zawsze da się rozpoznać te same historyczne zagrania. Zapewnij deterministyczną politykę scalania i pokaż ograniczenia; zachowaj liczniki źródłowe do weryfikacji.
- Dopasowuj pliki po trwałym ID/ścieżce i wiarygodnych danych pomocniczych. Niejednoznaczne dopasowania przedstaw do rozwiązania; nie łącz automatycznie różnych wersji utworu.
- Zachowaj istniejące ręczne oceny, komentarze i Cue, chyba że użytkownik wybierze ich zastąpienie. Wskaż, które dane wygrają przy scalaniu.
- Dla rekordbox XML przelicz ocenę `0/51/102/153/204/255` na brak oceny i 1–5 gwiazdek. Pozycje Cue odczytuj zgodnie z formatem; Memory Cue nie może przypadkowo stać się Hot Cue 1.
- Brak danych w źródle oznacza brak wartości, a nie dzisiejszą datę lub wymyśloną historię. Częściowy eksport sesji nie jest dowodem pełnej liczby odtworzeń utworu.
- Zapisz raport importu: źródło i wersja, przeniesione rekordy, pominięcia, konflikty i błędy. Długie zadanie działa w tle i można je anulować bez uszkodzenia biblioteki.

### 2D. Automatyczne skrzynki ocen

W drzewie biblioteki automatycznie utwórz grupę „Oceny” z pięcioma inteligentnymi skrzynkami: `★`, `★★`, `★★★`, `★★★★`, `★★★★★`. Każda obejmuje utwory **z całej kolekcji o dokładnie tej ocenie**. Dodaj także wygodny filtr „Bez oceny”.

Zmiana gwiazdek natychmiast aktualizuje przynależność i liczniki. To filtry bazy, a nie kopie plików muzycznych. Nie twórz duplikatów tych skrzynek przy każdym uruchomieniu. Zachowaj dotychczasowe skrzynki użytkownika i emoji w ich nazwach.

### 2E. Kolumna PREVIEW w głównej tabeli

Dodaj kolumnę „Podgląd / PREVIEW” zgodnie z czerwonym wyróżnieniem na `4.png`: mały waveform całego utworu w każdym widocznym wierszu tabeli, najlepiej przed tytułem. Na nim zaznacz zapisane Hot Cue w tej samej palecie i pozycjach co na decku oraz w TRACK INFO.

Ważne rozróżnienie techniczne: **pozycje Cue nie zawierają próbek potrzebnych do narysowania fali**. Zapisuj osobno analizę audio, np. zredukowane piki i dane pasm, oraz osobno Hot Cue; powiąż oba zestawy tym samym ID utworu. Po ponownym uruchomieniu odczytuj analizę i markery z trwałego magazynu/cache, bez konieczności ładowania utworu do decka.

Jeżeli analiza jeszcze nie istnieje, zleć ją w tle w ograniczonej kolejce i pokaż neutralny stan. Zachowaj wirtualizację tabeli; nie dekoduj całej kolekcji podczas rysowania wierszy. Zmiany Cue mają aktualizować wszystkie podglądy. Dla niezaznaczonych, nieodtwarzanych utworów nie pokazuj fikcyjnego playheada. Klikanie podglądu nie może przypadkowo uruchamiać odtwarzania na aktywnym decku.

## Punkt 3. Położenie suwaków PITCH BEND na A i C

Na lewych deckach **A oraz C** przesuń pionowy PITCH BEND na stronę jogu zwróconą do głównego miksera. Ma leżeć **pomiędzy jogiem a mikserem**.

Docelowy porządek na A/C, czytany od lewej do prawej: informacje i pady decka, obszar samplera, jog, PITCH BEND, główny mikser. Dokładne szerokości dopasuj do okna. Zachowaj poprawne działanie układu B/D.

To zmiana rozmieszczenia istniejącej kontrolki. Zachowaj chwilowy charakter pitch bend: po puszczeniu wraca do zera, nie zmienia na stałe tempa, działa z myszą i MIDI oraz zeruje się po utracie fokusu. Nie pomyl go z suwakiem stałego pitch/tempo.

## Punkt 4. Wygląd Auto Loop, Manual Loop i Sampler

Ujednolić panele wybierane z menu trybów padów, używając zwartej siatki prostokątnych komórek z `2(1).png` i `2.2.png`: grafit/czerń, cienkie separatory, czytelne napisy, cyjanowe oznaczenie aktywnej funkcji, wyraźne stany hover/press/active.

- **Auto Loop:** czytelne długości pętli, np. 1/2/4/8/16/32 beaty w zakresie obsługiwanym przez silnik, wyróżnienie aktualnej pętli, włącz/wyłącz, skrócenie i podwojenie oraz strzałki zmiany banku długości, jeśli potrzebne.
- **Manual Loop:** układ tabeli z przyciskami IN, OUT, OFF/EXIT i powrotem do pętli; wykorzystaj dotychczasową logikę. Dodaj sensowne ×2/÷2 tam, gdzie działa skalowanie pętli. Stan IN i aktywnej pętli musi być widoczny.
- **Sampler:** cztery sloty w stylu tabeli, powiązane z bankiem przeniesionym w punkcie 9. Widoki mają korzystać z tego samego stanu i jednego wyzwolenia audio.

Zrzut `2(1).png` pokazuje również BEATJUMP/KEY jako część obcego interfejsu. Jest wzorem organizacji panelu; nie dodawaj atrap nowych funkcji audio tylko po to, by powtórzyć wszystkie napisy. Każdy widoczny, aktywny przycisk ma mieć rzeczywiste działanie.

## Punkt 5. Większe przyciski PLAY i SYNC

Powiększ PLAY i SYNC na wszystkich aktywnych deckach. Mają wyglądać jak wyraźne, nowoczesne przyciski z większym obszarem kliknięcia, dobrym kontrastem i wspólnym stylem. Inspiracją są profesjonalne programy DJ; dopasuj rezultat do Prospekt DJ.

PLAY pokazuje trójkąt w stanie gotowości i rozpoznawalny stan pauzy podczas odtwarzania. Dodaj krótkie, płynne animacje najechania, naciśnięcia i zmiany stanu, np. delikatną zmianę obramowania i poświaty. SYNC sygnalizuje rzeczywiście wykonaną operację lub istniejący stan synchronizacji.

Nie pokazuj trwałego „zablokowanego SYNC”, jeśli obecny silnik wykonuje tylko jednorazowe dopasowanie tempa. Zmiana wyglądu nie może fałszywie sugerować nowej synchronizacji fazy beatów. Animacje nie mogą opóźniać transportu, powodować migotania ani wymuszać dużego obciążenia GPU. Dodaj opcję ograniczenia animacji w ustawieniach grafiki. Zachowaj skróty i obsługę MIDI.

## Punkt 6. RAM, VRAM i FPS w górnym pasku

Obok zegara i istniejących statystyk CPU/dysku dodaj:

- **RAM:** rzeczywiste użycie pamięci, np. użyte/całkowite GiB i procent. W tooltipie można dodać zużycie samej aplikacji; jednoznacznie podpisz, czego dotyczy liczba.
- **VRAM:** rzeczywiste wykorzystanie pamięci GPU, jeśli system/sterownik udostępnia te dane. Podpisz zakres: aplikacja lub urządzenie, pamięć dedykowana lub współdzielona. Przy iGPU lub braku obsługi pokaż „N/D”, zamiast wymyślonego zera.
- **FPS:** zmierzone klatki interfejsu na sekundę, wygładzone w krótkim oknie czasu. Użyj etykiety `FPS`; samo FPS już oznacza klatki na sekundę.

FPS ma wynikać z rzeczywistego tempa renderowania, a nie z zadanej wartości limitera. Odczyty systemowe odświeżaj w tle, np. raz na sekundę; sam licznik FPS może korzystać z lekkiego pomiaru czasu klatek. Nie odczytuj sterowników GPU ani systemowych statystyk z callbacku audio. Zachowaj obsługę Windows i Linux oraz zwięzłość nagłówka.

## Punkt 7. Główny mikser zgodny z referencją — NAJWYŻSZY PRIORYTET

**Kieruj się `1(2).png`.** Chodzi o styl i topologię kontrolek: małe czarne knoby z cienką wskazówką i kolorowym akcentem, wąskie pionowe fadery ze skalą, przyciski PFL, symetryczne paski kanałów, centralne mierniki i poziomy crossfader u dołu.

W układzie dwóch decków zastosuj tę kompozycję do A/B. W układzie czterech decków zapewnij analogiczny, niezależny mikser dla C/D. Litery kanałów i aktywne stany mają być czytelne.

### 7A. Dokładne zamiany i kolory

| Element z referencji / istniejącej aplikacji | Docelowy element | Kolor i działanie |
| --- | --- | --- |
| VOCAL, górny knob na zewnętrznej kolumnie | **HIGH** z dotychczasowego MIXER/EQ | `#28C800`, RGB 40, 200, 0; regulacja wysokich tonów. |
| INSTRU, środkowy knob | **MID** z MIXER/EQ | `#FFD700`, RGB 255, 215, 0; regulacja średnich tonów. |
| BEAT, dolny knob | **LOW** z MIXER/EQ | `#E60000`, RGB 230, 0, 0; regulacja niskich tonów. |
| FILTER w dolnej części kanału | **FILTER** z DJ-FX / FX RACK / 01 FILTER | Istniejący LPF/HPF; środek neutralny, lewa strona LPF, prawa HPF zgodnie z silnikiem. |
| GAIN nad pionowym faderem | **GAIN**, powiązany z dotychczasową funkcją MIXER / LEVEL | Jasna, czytelna wskazówka; właściwe sterowanie poziomem danego kanału. |
| Pionowy suwak kanału | Fader głośności A, B, C lub D | Wąski tor, wyraźny uchwyt i kreski skali. |
| PFL pod faderem | Rzeczywisty przycisk odsłuchu danego kanału | Cyjanowe/niebieskie podświetlenie tylko przy aktywnym PFL. |
| Dolny poziomy suwak | Istniejący crossfader | Zachowaj przypisanie A/C do lewej strony, B/D do prawej. |
| Powierzchnia miksera z knobami i suwakami | Grafitowe tło | **`#1A1A1A`, RGB 26, 26, 26**. |

Rozmieszczenie na lewym pasku kanału: zewnętrzna pionowa kolumna HIGH, MID, LOW, a niżej FILTER; bliżej osi miksera GAIN nad faderem kanału, pod nim PFL. Drugi pasek jest lustrzanym odbiciem. Zachowaj środkowy obszar na mierniki i odstęp między kanałami. Crossfader znajduje się pod paskami. Dopasuj szczegóły do referencji, zamiast układać wszystkie knoby w jednym poziomym rzędzie.

Kolory HIGH/MID/LOW dotyczą akcentów, wskazówek lub łuków knobów. Korpus pozostaje ciemny. Tło `#1A1A1A` stosuj do obszaru miksera, nie jako przypadkowe zastępstwo całego motywu.

Zastąp funkcje STEMS z referencji korektorem. W tej aktualizacji nie potrzebuję VOCAL/INSTRU/BEAT ani ich etykiet w nowym mikserze. Inne drobne oznaczenia na zdjęciu wykorzystaj tylko tam, gdzie odpowiadają realnym funkcjom Prospekt DJ.

### 7B. Powiązanie z istniejącym audio

HIGH/MID/LOW w głównym mikserze i w zakładce MIXER sterują **tymi samymi** parametrami EQ. FILTER w mikserze i w FX RACK steruje **tym samym** filtrem. Zmiana w jednym miejscu od razu aktualizuje drugie miejsce i stan MIDI. Nie dodawaj drugiego filtra w szeregu tylko dlatego, że jest drugi widok kontrolki.

W obecnej rewizji MIXER/LEVEL i pionowy fader korzystają z tego samego `DeckView.volume`. Rozstrzygnij to podczas integracji: jeśli GAIN i fader mają działać niezależnie jak w mikserze DJ, wyodrębnij neutralny gain przed EQ/faderem i pozostaw fader poziomu kanału. Domyślny nowy gain ma zachować poprzedni poziom dźwięku. Nie przedstawiaj dwóch kopii tej samej wartości jako dwóch niezależnych stopni regulacji. Zachowaj zgodność istniejących mapowań, dodając rozróżnione akcje lub migrację tam, gdzie jest potrzebna.

Przy regulacji zachowaj wygładzanie parametrów i limiter. Fader na minimum wycisza kanał niezależnie od gainu. PFL pozostaje prawdziwym odsłuchem, a mierniki pokazują rzeczywisty sygnał.

### 7C. MASTER, PHONES i REC w nagłówku

Przenieś globalne knoby **MASTER** i **PHONES** do górnego paska programu, obok **AUDIO ON**. W tym samym obszarze umieść **REC jako czerwoną kropkę**.

Zachowaj obecną funkcję nagrywania, wybór pliku, stan kończenia zapisu i obsługę błędów. W trakcie nagrywania czerwona kropka może delikatnie pulsować; kliknięcie zatrzymuje nagrywanie. Stan zapisu i tooltip mają być jednoznaczne. Nie twórz drugiego systemu nagrywania. MASTER, PHONES i REC są globalne i występują raz, także w trybie czterech decków.

### 7D. Wspólny styl małych kontrolek

Zbuduj współdzielone komponenty knobów i faderów. Mały korpus, np. około 24–28 logicznych px, powinien mieć wygodniejszy obszar interakcji i czytelne etykiety. Zachowaj precyzyjną regulację z Shift, dwuklik do wartości domyślnej, tooltip z wartością i obsługę klawiatury/MIDI. Knoby EQ mają neutralne 0 dB; FILTER neutralny środek. Wąskie suwaki mają wyraźne uchwyty i skalę.

Kompaktowość wizualna nie może spowodować nakładania hitboxów ani utrudnić regulacji. Zastosuj poprawne skalowanie DPI. Najpierw dopracuj tę sekcję, zanim uznasz zmianę wyglądu za zakończoną.

## Punkt 8. Profesjonalne okno ustawień

Rozbuduj ustawienia w spójne okno z lewą listą kategorii, wyszukiwaniem ustawień, wyraźnymi grupami, opisami i przywracaniem wartości domyślnych dla kategorii. Zastosuj konsekwentne odstępy, typografię, kontrolki i przewijanie.

Uwzględnij co najmniej:

| Kategoria | Rzeczywiste ustawienia |
| --- | --- |
| Ogólne | Język, zachowanie przy starcie, zapamiętywanie układu. |
| Wygląd i wydajność | Motyw, skala UI/czcionek, gęstość tabeli, animacje i ograniczony ruch, limit odświeżania, widoczność statystyk. |
| Decki i sterowanie | 2/4 decki, prezentacja jogów, czułość obsługiwanych kontrolek, istniejące skróty i MIDI. |
| Waveformy | Preset RGB, jasność, nasycenie/kontrast, wysokość, domyślny zoom i siatka, widoczność numerów/markerów. |
| Audio i odsłuch | Obsługiwane wyjścia MASTER/PFL, poziomy, dostępna częstotliwość i bufor/opóźnienie, diagnostyka wybranego backendu. |
| Nagrywanie | Istniejące formaty, folder zapisu i dostępność wymaganych kodeków. |
| Biblioteka i katalogowanie | Foldery, skanowanie, cache analiz/okładek, widoczność PREVIEW/TRACK INFO, kolumny i sortowanie, zasady liczenia odtworzeń. |
| Import i baza | Kreator czterech źródeł, raporty, rozwiązywanie ścieżek, kopia/odtworzenie własnych danych Cue, ocen, komentarzy i historii. |
| Sampler i automix | Parametry istniejących funkcji oraz konfiguracja nowego miejsca samplera. |
| Informacje | Wersja, licencje i źródła użytych komponentów, istniejące informacje o projekcie. |

Dla RGB waveformów dodaj zestaw działających presetów, np. **Standard, Żywe, Dynamiczne, Stonowane, Ciemniejsze, Monochromatyczne, Wysoki kontrast**. „Dynamiczne” może zmieniać intensywność zgodnie z rzeczywistymi danymi pasm i amplitudy, bez dekoracyjnej losowości. Udostępnij podgląd presetu i opcję własnej palety.

Zmiana palety waveformu nie zmienia dźwięku, kolorów Hot Cue ani wymaganych kolorów EQ. Nie wymaga ponownej analizy całej kolekcji, jeśli zapisane są odpowiednie dane pasm. Monochromatyczny waveform nie musi odbierać kolorów znacznikom Cue.

Każde aktywne ustawienie ma działać i zapisywać się między sesjami. Opcje zależne od backendu udostępniaj zgodnie z jego możliwościami. Zmiany urządzenia audio wymagające zatrzymania silnika zastosuj w kontrolowany sposób, zachowując sesję i obecne zabezpieczenia nagrywania.

## Punkt 9. Mniejsze knoby DJ-FX/MIXER i przeniesiony sampler

### 9A. Knoby w zakładkach

W zakładkach **DJ-FX** oraz **MIXER** zmniejsz knoby i nadaj im wspólny styl z punktu 7. Zachowaj ich parametry, zakresy, wartości, reset i działanie. Układ ma być kompaktowy, ale czytelny. Współdziel wartości z głównym mikserem tam, gdzie obie kontrolki dotyczą tej samej funkcji.

### 9B. Docelowe miejsce i układ samplera

Przenieś funkcje dużej zakładki samplera w **obszar zaznaczony na czerwono na `5.PNG`**, wewnątrz panelu decka. To zwarta przestrzeń między dotychczasową częścią padów a jogiem/suwakiem. Po zmianie położenia PITCH BEND na A/C zachowaj sens tej lokalizacji: sampler leży w informacyjnej części decka przed jogiem.

Wykorzystaj tabelkowy styl `2(1).png`. Przyjmij kompaktową siatkę **2 × 2 dla czterech slotów** jako decyzję układową dopasowaną do miejsca. Dla każdego slotu pokaż numer, krótką nazwę sampla i stan. Pełną nazwę, rozmiar pliku i czas udostępnij w tooltipie lub rozwinięciu, aby panel pozostał mały.

| Slot samplera | Dokładny kolor |
| --- | --- |
| 1 | `#1A1A1A` |
| 2 | `#E60000` |
| 3 | `#FFD700` |
| 4 | `#28C800` |

Użyj tych kolorów dla odpowiednich załadowanych slotów. Pierwszy grafitowy slot musi pozostać rozpoznawalny na tle przez jasną etykietę i obramowanie. Dobierz kontrast napisów, szczególnie na złotym tle. Pusty, załadowany, wskazany i odtwarzany slot mają odróżnialne stany. Numer i nazwa uzupełniają kolor.

Przenieś wszystkie dotychczasowe funkcje: ładowanie własnego pliku, nazwę i rozmiar, wyzwalanie, usuwanie, regulację poziomu oraz pobieranie fragmentu decka, jeśli jest obecnie dostępne. Zachowaj istniejącą obsługę MIDI i skrótów.

Obecny sampler ma cztery współdzielone sloty. Zachowaj ten bank i jego działanie. Jeśli nowy panel jest widoczny w kilku deckach, są to widoki tego samego banku; jedno naciśnięcie nie może odtworzyć sampla kilka razy. Tryb SAMPLER w menu padów również korzysta z tego stanu. Po przeniesieniu usuń stary duży panel lub przebuduj nawigację tak, by nie zajmował nadal dawnego miejsca i nie pozostawiał martwej zakładki.

## Punkt 10. Wszystkie języki urzędowe Unii Europejskiej

Dodaj pełne tłumaczenie interfejsu na **24 języki urzędowe UE**, zachowując pozostałe języki już dostępne w programie.

| Język | Kod | Język | Kod |
| --- | --- | --- | --- |
| Angielski | `en` | Bułgarski | `bg` |
| Chorwacki | `hr` | Czeski | `cs` |
| Duński | `da` | Estoński | `et` |
| Fiński | `fi` | Francuski | `fr` |
| Grecki | `el` | Hiszpański | `es` |
| Irlandzki | `ga` | Litewski | `lt` |
| Łotewski | `lv` | Maltański | `mt` |
| Niderlandzki | `nl` | Niemiecki | `de` |
| Polski | `pl` | Portugalski | `pt` |
| Rumuński | `ro` | Słowacki | `sk` |
| Słoweński | `sl` | Szwedzki | `sv` |
| Węgierski | `hu` | Włoski | `it` |

Tłumaczenie ma obejmować menu, ustawienia, TRACK INFO, historię, importery, błędy, tooltipy, nowe skrzynki ocen, nazwy kolumn i wszystkie nowe przepływy tej aktualizacji. Same nazwy języków w rozwijanej liście nie oznaczają ukończenia tłumaczenia.

Zachowaj prawidłowe parametry komunikatów, odmianę liczby mnogiej tam, gdzie jest potrzebna, i formatowanie dat/liczb zgodne z lokalizacją. Nie tłumacz tytułów utworów, komentarzy użytkownika, nazw plików i własnych skrzynek. Zachowaj specjalistyczne terminy DJ tam, gdzie są naturalnie używane.

Zaktualizuj obecny system katalogów i testy: aktualny kod ma tablicę 10 języków i katalogi indeksowane pozycją. Dodanie języków nie może przestawić starych tłumaczeń ani zmienić zapisanego wyboru języka. Sprawdź kompletność kluczy i renderowanie znaków łacińskich, greckich, cyrylicy oraz znaków diakrytycznych, w tym maltańskich i irlandzkich. Zachowaj dotychczasową obsługę emoji i języków spoza UE.

## Punkt 11. OpenFadeR jako pomoc implementacyjna

Przeanalizuj https://github.com/OpenRsG/OpenFadeR i wykorzystaj pasujące rozwiązania jako materiał pomocniczy. Nie przenoś całego obcego interfejsu ani jego niezwiązanych modułów. Zachowaj Rust i egui/eframe używane w Prospekt DJ; OpenFadeR stosuje inny framework UI, Slint.

Przydatne miejsca znalezione podczas analizy OpenFadeR:

- `src/store/history.rs` — trwała historia i grupowanie sesji.
- `src/store/track.rs`, `src/store/mod.rs` — magazyn danych utworów; sprawdź przed adaptacją.
- `src/library/import.rs`, `src/library/import_report.rs` — organizacja importowania i raportu.
- `compatibility/serato/` — czytniki kolekcji, skrzynek i danych markerów; osobno zweryfikuj historię.
- `compatibility/virtualdj/` — obsługa formatu biblioteki i metadanych.
- `compatibility/rekordbox/xml/`, `compatibility/rekordbox/db/`, `compatibility/rekordbox/common/` — różne źródła rekordbox i ich zakresy danych.
- `src/app/mixer/`, `src/app/sampler/` — przykłady organizacji logiki kontrolek.
- `docs/plugins.md` i `README.md` — zakres integracji oraz rozróżnienie bibliotek od mapowań MIDI.

OpenFadeR nie jest dowodem, że wszystkie wymagane importy historii są już gotowe. W jego opisie Mixxx dotyczy integracji kontrolerów, więc importer biblioteki Mixxx do tej aktualizacji trzeba zweryfikować i wykonać oddzielnie.

Przy wykorzystaniu fragmentów kodu zachowaj wymagane informacje o autorach i licencji oraz opisz zapożyczenia w dokumentacji projektu. Dopasuj kod do własnego modelu danych i testów.

## Wskazówki integracyjne dla aktualnego Prospekt DJ

| Moduł lub funkcja | Związek z aktualizacją |
| --- | --- |
| `src/ui.rs`: `deck_ui`, `jog_panel`, `pitch_bend` | Zmiana położenia suwaka A/C oraz miejsce samplera. |
| `src/ui.rs`: `mini_mixer`, `mixer_ui`, `effects_ui`, `sampler_ui`, `dock_ui` | Przebudowa miksera, małe knoby i przeniesienie samplera. |
| `src/ui.rs`: `track_preview`, `browser_table`, `settings_ui` | TRACK INFO, PREVIEW, oceny, historia i nowe ustawienia. |
| `src/engine.rs`: `DeckView.hot`, polecenia `HotCue`, `Pad`, `Load` | Trwałe Cue, ich przywracanie i poprawna kolejność aktualizacji. |
| `src/engine.rs`, `src/dsp.rs`, `src/effects.rs` | Powiązanie gain/fader, EQ, filtra, PFL, poziomów i rzeczywistych stanów kontrolek. |
| `src/library.rs` | Obecny katalog JSON, schemat 2; nowe trwałe dane utworu, filtry ocen, komentarze i import. |
| `src/media.rs`, `src/music_view.rs` | Analiza waveformów, wspólna geometria markerów, cache miniatur i presety kolorów. |
| `src/artwork.rs`, `src/emoji.rs`, `src/folder_tree.rs` | Zachowanie istniejących okładek, emoji i drzewa biblioteki. |
| `src/telemetry.rs` | Rozszerzenie odczytów CPU/dysku o RAM i dostępne dane VRAM; FPS mierzone przy renderowaniu. |
| `src/widgets.rs` | Wspólne małe knoby, fadery i stany przycisków. |
| `src/settings.rs`, `src/i18n.rs`, `assets/translations.json` | Migracja ustawień i pełne katalogi językowe. |
| `tests/v23.rs`, `tests/library.rs`, `tests/core.rs` | Zachowanie istniejących regresji i testy nowej logiki. |

Odczyty i zapisy bazy, importery, analiza plików oraz statystyki systemowe mają działać poza callbackiem audio i bez synchronicznego blokowania UI. Wykorzystaj istniejące kolejki i komunikaty. Zapis bazy nie może zależeć wyłącznie od sporadycznie odczytanego, potencjalnie starego snapshotu interfejsu. Zachowaj kolejność zdarzeń z przypisaniem do konkretnego utworu i generacji ładowania.

## Warunki ukończenia i sprawdzenie

1. **Punkt 7:** pokaż faktycznie przebudowany mikser A/B i C/D odpowiadający `1(2).png`, właściwe kolory i rozmieszczenie, działające powiązania EQ/filtra, fadery i PFL oraz MASTER/PHONES/REC w nagłówku. Samo zmniejszenie starych knobów nie kończy tego punktu.
2. Cue ustawione m.in. w 0 sekundzie oraz w slotach 1, 3 i 8 wracają po zmianie utworu, zmianie decka i restarcie. Usunięte nie wracają. Sprawdź też szybkie ładowanie kolejnych utworów i edycję tego samego pliku na dwóch deckach.
3. TRACK INFO i PREVIEW korzystają z zapisanej analizy oraz tych samych Cue, również dla niezaładowanego utworu. Zmiana Cue aktualizuje wszystkie widoki bez ponownego dekodowania całej kolekcji.
4. Oceny i komentarze przetrwają restart i ponowne indeksowanie. Skrzynki ocen zawierają dokładnie właściwe utwory; Ulubione pozostają odrębną funkcją.
5. Pierwsze i ostatnie odtworzenie, licznik i poprzednia sesja mają poprawną semantykę. Testuj pauzę/wznowienie, przewijanie, Hot Cue, pętlę, krótki utwór, PFL, automix i C/D. Nie zwiększaj licznika na każde kliknięcie PLAY.
6. Każdy z czterech importerów ma sprawdzony wariant formatu i raport dostępnych pól. Powtórny import nie dubluje zdarzeń. Niepełne dane, daty dzienne, polskie znaki w ścieżkach, URI, zmiana dysku i konflikty utworów są obsłużone.
7. PITCH BEND na A/C znajduje się po stronie miksera i wraca do zera po puszczeniu lub utracie fokusu. Stałe tempo pozostaje poprawne.
8. Panele Auto Loop i Manual Loop mają rzeczywiste, działające przyciski i spójny styl. Sampler mieści się w miejscu z `5.PNG`, ma cztery wymagane kolory i wszystkie dotychczasowe funkcje; pojedynczy klik wyzwala jedno odtworzenie.
9. Większe PLAY/SYNC reagują natychmiast i pokazują prawdziwy stan. Wpisywanie komentarzy, nazw i danych w kreatorze nie uruchamia skrótów decka.
10. RAM i FPS są mierzone. VRAM przedstawia prawdziwy odczyt albo uczciwe N/D. Pasek nagłówka nie nachodzi na MASTER/PHONES/REC, zegar i AUDIO ON.
11. Presety waveformów zmieniają faktyczny wygląd, zapisują się i nie zmieniają palety Cue ani kolorów EQ. Ustawienia działają po restarcie i migracji starszego pliku konfiguracji.
12. Wszystkie 24 języki UE mają komplet kluczy, prawidłowe parametry i działające fonty. Zachowane języki spoza UE oraz wcześniejszy wybór użytkownika nadal działają.
13. Sprawdź układ 2/4 decków, co najmniej 1280 × 720 i 1920 × 1080 oraz różne DPI. Import i analiza w tle nie mogą blokować sterowania ani pogarszać stabilności audio.

Dodaj testy trwałego zapisu, migracji, deduplikacji historii, dopasowań importu i synchronizacji danych. Importery sprawdzaj na reprezentatywnych fiksturach oraz udokumentowanych wariantach źródeł. Wygląd potwierdź uruchomieniem i zrzutami interfejsu, jeśli środowisko pozwala. Zachowaj istniejące testy i uruchom wymagane kontrole, w szczególności:

```sh
cargo fmt --all -- --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked --release
```

Na zakończenie przedstaw podsumowanie **punkt po punkcie 1–11**: co wdrożono, co sprawdzono i jakie konkretne ograniczenia pozostały. Dla importów podaj obsługiwane wersje/formaty i pola. Nie deklaruj testu fizycznego kontrolera ani danych VRAM, jeśli nie zostały rzeczywiście sprawdzone.

## Źródła pomocnicze do weryfikacji formatów

- Prospekt DJ: https://github.com/prospektdj/prospektdj
- OpenFadeR: https://github.com/OpenRsG/OpenFadeR — analiza pomocnicza dotyczyła `main`, drzewa `506add6f8c45604aeaa15a7570460bab3cd838da`.
- Serato, historia sesji: https://support.serato.com/hc/en-us/articles/223455687-History
- VirtualDJ, pola biblioteki: https://virtualdj.com/manuals/virtualdj/interface/browser/filelist.html
- rekordbox, specyfikacja eksportu XML: https://cdn.rekordbox.com/files/20200410160904/xml_format_list.pdf — zweryfikuj zakres względem konkretnej wersji eksportu użytkownika.
- Mixxx, biblioteka i historia: https://manual.mixxx.org/2.5/en/chapters/library.html
- Mixxx, wersjonowany schemat bazy: https://github.com/mixxxdj/mixxx/blob/main/res/schema.xml
- Oficjalna lista języków UE: https://european-union.europa.eu/principles-countries-history/languages_en
