# Live recommendations

## 🇵🇱 Polski

Włącz „Rekomendacje live” w bibliotece. Czerwony pogrubiony pasek pokazuje lokalne propozycje; strzałki przeglądają do 50 kandydatów, a kliknięcie wskazuje utwór w bibliotece. Przyciski A/B oraz C/D w trybie czterech deków ładują utwór bez automatycznego startu.

Interwał w Ustawienia → Ogólne wynosi 1–3600 s, domyślnie 60 s, i jest zapisywany. Ręczne przełączenie zeruje timer. Zmiana granego utworu/tempa unieważnia propozycję. Co 3 s wątek ocenia cały zaindeksowany katalog niezależnie od filtra, folderu i BANGER BOX. Wszystkie grające deki mają równy udział. Załadowane i brakujące pliki są pomijane.

BPM (także pół/podwójne), zgodność tonacji i gatunek mają wagi 60/30/10. Brakujące dane nie są zgadywane. Bez odtwarzania brak propozycji. Jest to lokalna analiza metadanych, bez sieci, uczenia neuronowego i dekodowania całej biblioteki podczas występu. Uzupełnij tagi BPM/key/genre.

Każdy utwór może być pokazany dwa razy na cykl. Pamięć sesji przetrwa zmiany deków/tempa i przełącznika; liczone są prezentacje, nie klatki UI. Po wyczerpaniu puli na 4 s pojawia się komunikat rozpoczęcia od nowa. Sam brak zgodnych metadanych nie powoduje pętli restartów.

Prawy panel (okna od 1200 px) ma Track Info i Historię rekomendacji: ostatnie 10 propozycji, czas, tytuł, wykonawcę i szczegóły dopasowania. Statystyki pokazują zadania, odrzucone wyniki, liczbę ocen, wielkość puli, czas ostatni/średni, prezentacje, unikalne utwory i cykl. Odświeżenie rankingu nie dopisuje pozycji historii. Fonty systemowe mają otwarte zamienniki.

## 🇬🇧 English

Enable **Rekomendacje live** in the library toolbar. The red Arial Bold strip under the track table shows local suggestions; arrows browse up to 50 candidates, and clicking a suggestion selects it in the complete library. The larger Polish strip includes A/B load buttons (plus C/D in four-deck mode). Each button loads the suggested track through the normal manual-loading path, without starting playback automatically.

The displayed suggestion advances automatically at the interval chosen in **Settings → General**, from **1 to 3600 seconds**, default **60 seconds**. The value is saved between launches. Manual arrow navigation restarts this timer. A change of playing track/tempo invalidates the current suggestion immediately; normal re-ranking preserves the displayed candidate until rotation unless it is no longer eligible.

Every three seconds, a background worker ranks the entire indexed catalog, independently of the search, current folder or BANGER BOX. All playing decks contribute equally. Tracks already loaded on any visible deck and entries marked missing are excluded. Changing playback or deck tempo invalidates the previous suggestions.

Ranking uses indexed BPM (including half/double-time matches), compatible Camelot/musical keys and exact genre matches, weighted 60/30/10. Missing or invalid metadata is not invented. No playback means no suggestions. This is metadata analysis, not a neural model or automatic acoustic key detection; fill in BPM/key/genre tags to improve coverage. It does not decode the entire collection during a performance, use a network service or send library data elsewhere.

The option is persisted in settings. Artist, title and matching reasons appear in the strip/tooltip. Microsoft fonts are read from the operating system; an installed open substitute is used when Arial Bold is unavailable.

A track can be presented at most **twice per recommendation cycle**. Session memory survives deck/tempo changes and toggling recommendations, and only counts distinct presentations, not UI frames or ranking refreshes. Less frequently shown alternatives are preferred. When all otherwise eligible candidates reach the limit, the library displays **Rekomendacje: rozpoczynam generowanie propozycji od nowa…** for four seconds and starts a fresh cycle. A lack of metadata matches alone does not trigger endless restarts.

The library’s right sidebar has **Track Info** and **Historia rekomendacji** tabs (available in windows at least 1200 logical pixels wide). Recommendation history shows the ten most recently displayed proposals, newest first, with time/title/artist and matching details on hover. The list and statistics live for the application session and survive cycle resets. Statistics show completed ranking jobs, stale jobs, catalog entries considered across jobs, last pool size, measured last/average ranking duration, actual presentations, unique tracks and the current cycle. Background ranking refreshes do not add presentation-history rows.
