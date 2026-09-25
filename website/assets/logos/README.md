# OrchidDB logo

`orchiddb.svg` is the faithful Orchid kamon vector master. Transparent PNGs use
the same geometry and #352330 ink at 50, 128, 256, 512, and 1024 pixels:
`orchiddb-50.png`, `orchiddb-128.png`, `orchiddb-256.png`, `orchiddb-512.png`,
and `orchiddb-1024.png`. `orchiddb.png` is an identical copy of the 1024px version,
served at https://orchiddb.com/assets/logos/orchiddb.png.

Regenerate with `node website/scripts/export-logo-pngs.mjs` using Playwright.
The script also accepts `PLAYWRIGHT_MODULE` and `CHROME_PATH`, matching the site
browser-check setup, and verifies dimensions, transparency, and ink color.
`orchiddb-ink.png` is the separate approved brush artwork used by the hero.

# Technology logos

Official project artwork, downloaded unmodified on 2026-09-25. These marks
identify the technologies discussed on the site; they are not sponsor logos.

| Asset | Source |
| --- | --- |
| `rust.svg` | https://rust-lang.org/static/images/rust-logo-blk.svg |
| `duckdb.svg` | https://duckdb.org/images/designmanual/DuckDB_inline-lightmode.svg |
| `datafusion.svg` | https://datafusion.apache.org/_static/original.svg |
| `arrow.png` | https://arrow.apache.org/img/arrow-logo_horizontal_black-txt_white-bg.png |
| `iceberg.svg` | https://iceberg.apache.org/assets/images/Iceberg-logo.svg |
| `postgresql.png` | https://www.postgresql.org/media/img/about/press/elephant.png |

Rust and PostgreSQL marks belong to their respective projects. Apache project
marks belong to the Apache Software Foundation. DuckDB is a registered
trademark; see https://duckdb.org/design/ for original artwork and guidelines.

The source diagram shows external data exposed through DuckDB extensions and
connections. It does not claim direct native execution against every database.

Python, TypeScript, and Java artwork: [Devicon v2.17.0](https://github.com/devicons/devicon/tree/v2.17.0/icons), distributed under the [MIT license](https://github.com/devicons/devicon/blob/v2.17.0/LICENSE). Logos remain the property of their respective owners.
