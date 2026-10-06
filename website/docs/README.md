# Documentation

The book uses stock mdBook 0.5.4. Install with `bash website/docs/install-mdbook.sh`
or set `MDBOOK` to an existing executable. From the repository root:

```sh
python3 website/docs/build.py
python3 website/docs/check.py
python3 -m http.server 5321 --directory website/docs/dist
```

Edit chapters in `content/` and navigation in `content/SUMMARY.md`. Keep the standard
mdBook theme. The wrapper copies tutorial downloads and committed extension
conformance evidence, then writes a sitemap and text index. It runs no test engines.
The checker validates local links, anchors, downloads, and search coverage.
With a local preview running, `node website/docs/browser-check.mjs` checks desktop
and mobile navigation, search, and code copying; `node website/docs/conformance-check.mjs`
validates evidence downloads and artifact attribution. Set `PLAYWRIGHT_MODULE` and
`CHROME_PATH` when using an existing local Playwright/Chrome installation.

All builds and checks run locally. The generated `dist/` is ignored by Git.
Publication is separate: `AWS_PROFILE=personal bash website/docs/deploy.sh` uploads
the locally built documentation. Do not deploy as part of an ordinary docs edit.
The landing page is maintained separately. Legacy comparison renderer sources and
historical peer evidence remain developer references; the current book reports the
DuckDB extension's own pinned conformance results.
