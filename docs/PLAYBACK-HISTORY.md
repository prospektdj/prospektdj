# File-backed session history

## 🇵🇱 Polski

„Historia grania” w lewym panelu zawiera datowane sesje z kolejnymi odtworzeniami, czasem, dekiem A–D i ładowaniem. Powtórzenia są osobnymi wierszami, wyszukiwarka filtruje sesję, a brakujące pliki pozostają na liście. Muzyka źródłowa nie jest modyfikowana.

Po progu historii MASTER (domyślnie 30 s lub połowa krótkiego utworu) wątek bazy dopisuje rekord UTF-8 do history/<sesja>/history.jsonl; session.json zawiera etykietę i czas. Samo załadowanie, PFL i wyciszone granie nie liczą się. Restart audio zachowuje sesję, restart programu tworzy nową. Zapis/synchronizacja nie odbywa się w callbacku audio; błędy są zgłaszane.

Nazwy folderów YYYY-MM-DD-HH-MM-SS-PID są zgodne z Windows, etykieta YYYY-MM-DD-HH:MM. Skan co 2 s. Brakujące szczegóły historycznych danych nie są wymyślane. AUTOMIX obok REC otwiera ustawienia start/stop, następny, kolejka, czas przejścia, losowanie, ulubione, powtarzanie i kształt crossfadera. AutoMix działa na A/B; dostępne utwory sesji mogą być kolejką.

## 🇬🇧 English

The left library manager contains **Historia grania** with dated session folders. Select a session to see its chronological plays, time, deck A–D, title/artist and load buttons. Repeated plays remain separate rows. The search box filters the session; removed or unavailable source files remain listed. No original music files or existing metadata collections are changed.

When a play reaches the configured MASTER history threshold (default 30 seconds, or half the duration for short tracks), the existing database writer also appends a UTF-8 JSON record to `history/<session-id>/history.jsonl` alongside the app configuration. `session.json` stores the display label/time. PFL-only/muted playback and simply loading a track do not count. An audio-device restart keeps the same application-session folder; restarting the application starts a new session. Files are written and synced by the database worker, never by the audio callback. Writer failures are reported through the existing database error channel.

Folder names use `YYYY-MM-DD-HH-MM-SS-PID` for Windows compatibility and uniqueness. The library label uses `YYYY-MM-DD-HH:MM`. Sessions are scanned in the background every two seconds. Current playback is visible after the threshold and refresh. Earlier database-only plays are retained in the existing recent/frequent views; their original session details are not invented or retroactively exported.

The bottom AutoMix bar is removed. **AutoMix** beside **REC** opens Settings → Sampler and automix, containing start/stop, next, queue status, fade time, shuffle, favorites, repeat and crossfade shape. Clicking the top button only opens settings. AutoMix remains A/B; the available tracks of a selected history session can serve as its queue.
