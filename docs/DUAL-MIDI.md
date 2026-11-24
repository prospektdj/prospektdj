# Dwa kontrolery MIDI

## 🇵🇱 Polski

W **Ustawienia → MIDI** można utrzymywać dwa równoczesne połączenia wejściowe:

1. Kontroler 1: dotychczasowy wybór wejścia i mapowanie główne.
2. Kontroler 2: osobny wybór wejścia, przyciski połączenia/rozłączenia i wybór profilu z bazy mapowań.

Każde wejście ma własną ograniczoną kolejkę komunikatów i osobny bank mapowań. Identyczne numery Note/CC w obu urządzeniach są rozpoznawane oddzielnie. W MIDI Learn należy wybrać **Kontroler 1** albo **Kontroler 2**; lista przypisań i ich usuwanie dotyczą wybranego banku. Własne mapowania obu banków są zapisywane w settings.json. Dotychczasowy import/eksport profili dotyczy mapowania głównego; dla drugiego urządzenia można zastosować profil z listy lub MIDI Learn.

Po odłączeniu drugiego kontrolera pierwszy pozostaje aktywny. Ponowne podłączenie w tej samej sesji jest wykrywane według nazwy portu. Użyj różnych wejść MIDI; program blokuje otwarcie tego samego portu w obu slotach. Dwa urządzenia o identycznych, nierozróżnialnych nazwach portów wymagają nadania im rozróżnialnych nazw przez system/sterownik. Po ponownym uruchomieniu aplikacji wejścia należy połączyć ponownie.

Test automatyczny obejmuje rozdzielenie tych samych komunikatów między banki, niezależną edycję i zapis oraz zgodność starszych ustawień. Kompilacja i kontrola statyczna nie zastępują testu z dwoma fizycznymi kontrolerami. Sprzętowe LED/wyjście MIDI pozostają poza zakresem tej funkcji.


## 🇬🇧 English

Settings → MIDI supports two simultaneous input connections, each with a bounded queue and independent mapping bank. Identical Note/CC numbers remain distinct. MIDI Learn edits the selected Controller 1/2 bank. Both banks persist in settings.json; file import/export applies to the primary mapping, while the second accepts a catalog profile or Learn. Disconnecting one leaves the other connected; reconnect uses port names within the session. The same port cannot occupy both slots, and indistinguishable names must be renamed by the system. Connections are reopened after app restart. Routing and persistence tests do not replace physical dual-controller tests; LED/MIDI output is not implemented.
