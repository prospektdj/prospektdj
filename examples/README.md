# Przykłady / Examples

## 🇵🇱 Polski

`convert_controllers.rs` odbudowuje katalog profili z dostarczonego katalogu mapowań: `cargo run --locked --example convert_controllers -- /ścieżka/do/mapowań`. Skrypt konwertuje obsługiwane deklaracje; nie wykonuje zewnętrznych skryptów i zachowuje informacje o autorach. Wynik wymaga sprawdzenia przed zastąpieniem katalogu w assets.

## 🇬🇧 English

`convert_controllers.rs` rebuilds the profile catalog from a supplied mapping directory: `cargo run --locked --example convert_controllers -- /path/to/mappings`. It converts supported declarations without executing external scripts and retains author metadata. Review generated output before replacing the assets catalog.
