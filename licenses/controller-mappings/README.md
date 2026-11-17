# Controller mapping data and attribution

The user supplied an archive via Google Drive (file ID 1NBA9za5T-pl3jq8KzYnu5SOhcgNmTayN). Archive SHA-256: 72c183de7ccb389cfa038bacbd508a3b379c630fa428134507618383b9577db2.

The converted catalog contains 334 named profiles: 177 OpenFadeRMapping XML files, 156 Mixxx XML files (including 10 HID descriptors), and one OpenDJ declarative mapping. The catalog contains 20,854 recognized MIDI bindings; 289 profiles have at least one usable binding. A profile is not a claim of full hardware compatibility.

OpenFadeR declarative mapping data is distributed under AGPL-3.0-or-later, matching the source project license at https://github.com/OpenRsG/OpenFadeR/blob/main/LICENSE. The license text is included as AGPL-3.0.txt. The Prospekt DJ 2.2 combined distribution is AGPL-3.0-or-later. The independently implemented original Rust application was previously released under GPL-3.0-or-later; earlier releases retain their license. No OpenFadeR engine or application scripts are incorporated.

According to the supplied archive README, the Mixxx mappings came from Mixxx res/controllers at commit 0fffd3d0d07b755c34c1730f7b87afadd8f2be19 (2026-10-11), with their original authors and GPL-2.0-or-later licensing. GPL-2.txt is included. Original author names are retained in each converted profile when supplied by the XML metadata.

The OpenDJ mapping identifies its author as openDJ; only recognized declarative input/action pairs were converted.

Conversion preserves MIDI channel/status/note/controller numbers, known direct actions, relative encoder modes and inversion where explicitly declared. Unsupported script bindings, HID transport, 14-bit controls, layering and ambiguous duplicate inputs are recorded as compatibility notes. Source filename, author and schema accompany each profile. JavaScript files from the archive are not bundled or executed.

The conversion/import implementation is new Rust code in src/controller_import.rs. The catalog can be rebuilt from the extracted user archive using:

```sh
cargo run --example convert_controllers -- /path/to/extracted/mappings
```
