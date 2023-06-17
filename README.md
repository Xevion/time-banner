# time-banner

My first Rust project, intended to offer a simple way to display the current time relative, in an image format.

## Planned Features

- Dynamic light/dark mode
    - Via Query Parameters for Raster or SVG
    - Via CSS for SVG
- Relative or Absolute Format
- Dynamic Formats
- Caching Abilities
    - Relative caching for up to 59 seconds, purged on the minute
    - Absolute caching for up to 50MB, purged on a LRU basis
- Flexible & Dynamic Browser API
    - Allow users to play with format in numerous ways to query API
    - Examples
        - `/svg/2023-06-14-3PM-CST`
        - `2023-06-14-3PM-CST.svg`
        - `/jpeg/2023.06.14.33` (14th of June, 2023, 2:33 PM UTC)
        - `/jpeg/2023.06.14.33T-5` (14th of June, 2023, 2:33 PM UTC-5)