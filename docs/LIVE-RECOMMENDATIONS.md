# Live recommendations

Enable **Rekomendacje live** in the library toolbar. The red Arial Bold strip under the track table shows local suggestions; arrows browse up to 50 candidates, and clicking a suggestion selects it in the complete library. The larger Polish strip includes A/B load buttons (plus C/D in four-deck mode). Each button loads the suggested track through the normal manual-loading path, without starting playback automatically.

Every three seconds, a background worker ranks the entire indexed catalog, independently of the search, current folder or BANGER BOX. All playing decks contribute equally. Tracks already loaded on any visible deck and entries marked missing are excluded. Changing playback or deck tempo invalidates the previous suggestions.

Ranking uses indexed BPM (including half/double-time matches), compatible Camelot/musical keys and exact genre matches, weighted 60/30/10. Missing or invalid metadata is not invented. No playback means no suggestions. This is metadata analysis, not a neural model or automatic acoustic key detection; fill in BPM/key/genre tags to improve coverage. It does not decode the entire collection during a performance, use a network service or send library data elsewhere.

The option is persisted in settings. Artist, title and matching reasons appear in the strip/tooltip. Microsoft fonts are read from the operating system; an installed open substitute is used when Arial Bold is unavailable.
