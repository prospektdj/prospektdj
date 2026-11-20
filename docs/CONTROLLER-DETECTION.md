# Controller discovery

The app checks MIDI input ports every two seconds on a background thread, including at startup. Newly seen ports appear in **ODNALEZIONO URZĄDZENIE — KONTROLER**. Disconnect/reconnect uses the reported port name, rather than a stale enumeration index. A manual disconnect stays disconnected until explicitly reconnected or plugged in again.

The model is matched locally against the installed controller profiles. Case, spaces and punctuation are normalized; generic manufacturer names do not count as model identification. Empty/unsupported mappings are excluded. One unambiguous usable match connects and applies automatically if no controller is connected. Several variants require a selection, and an already connected controller is never replaced automatically. Suggested profiles are marked with a star; all usable profiles remain available for manual selection.

The dialog displays the driver-provided name and the selected profile's manufacturer/model, connection errors, and a link to MIDI settings/Learn. It does not claim USB identity verification or install hardware drivers. No web lookup is required and no device information is sent online. Different physical devices reporting the same MIDI port name require manual input selection.

Automated tests cover model matching, misleading prefixes, unsupported profiles and ambiguous variants. Physical USB/MIDI hot-plug testing requires an attached controller.
