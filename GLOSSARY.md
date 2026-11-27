# Internet Radio

A personal radio listening application for finding stations and returning to favorites.

## Language

**Station**:
A radio service listed in Radio Browser that offers a live audio stream.
_Avoid_: Channel

**Stream**:
The live audio feed offered by a station. Its address can change without making
the station a different station.

**Favorite**:
A station the listener has saved for easy access later.
_Avoid_: Bookmark, playlist entry

**Track metadata**:
Information supplied by a station about what is currently playing, when available.
_Avoid_: Guaranteed song identification

**Popularity**:
A station's click count within the preceding 24 hours as reported by Radio Browser,
used to order search results. It represents reported playback selections rather than
concurrent listeners.
