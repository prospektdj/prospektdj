# Historia rekomendacji TXT

## 🇵🇱 Polski

Każda nowa wyświetlona rekomendacja jest dopisywana do `recommendations/recommendations-RRRR-MM-DD.txt` w katalogu danych programu (obok EXE w przenośnej wersji Windows). Wpis zawiera czas ze strefą, wykonawcę, tytuł, wynik dopasowania, powód i ścieżkę pliku. Istniejące pliki są uzupełniane po ponownym uruchomieniu. Zapisy wykonuje osobny wątek, bez operacji dyskowych w silniku audio.

W ustawieniach znajduje się domyślnie włączona opcja **Zapisuj historię rekomendacji do plików TXT**. Wyłączenie zatrzymuje nowe wpisy; nie usuwa zapisanej historii i nie wpływa na rekomendacje na żywo ani podręczną listę ostatnich 10 propozycji.

Retencja jest liczona według daty dziennego pliku. Przy starcie oraz co godzinę działania program usuwa wyłącznie własne pliki `recommendations-RRRR-MM-DD.txt` z datą starszą niż 30 dni. Pozostałe pliki, foldery i dowiązania pozostają bez zmian. Gdy aplikacja jest zamknięta, czyszczenie nastąpi przy następnym uruchomieniu; nie jest instalowana usługa systemowa.


## 🇬🇧 English

Each newly displayed recommendation appends a timestamp, artist, title, score, reason and path to recommendations/recommendations-YYYY-MM-DD.txt in the shared app data directory. Files append across restarts; a dedicated worker performs writes outside audio callbacks. Saving is enabled by default and can be disabled in settings without deleting existing files or changing live ranking/ten-item session history. At startup and hourly while running, only owned dated files older than 30 calendar days are removed. Other files/directories/symlinks remain intact. No background system service runs when the app is closed.
