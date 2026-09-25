# Crabgraph documentation

The guides at https://docs.crabgraph.net/ use **mdBook 0.5.4 with the stock
theme**. No custom guide templates, stylesheets, or JavaScript are loaded.
mdBook supplies the sidebar, search, themes, code copying, and print view.

The detailed conformance explorer is a separate static report at
`conformance-report.html`, linked from the book. It retains the existing
feature filters, individual results, and evidence downloads. Its CSS and
JavaScript do not load in the book.

## Install, build, and preview

Install the pinned mdBook binary on macOS or Linux:

```sh
bash website/scripts/install-mdbook.sh
export PATH="$HOME/.local/bin:$PATH"
```

Alternatively, run `cargo install mdbook --version 0.5.4 --locked`.
Python 3.10+ is needed to package the evidence report; no Python packages
are required.

From the repository root:

```sh
python3 website/docs/build.py
python3 website/docs/check.py
python3 -m http.server 5321 --directory website/docs/dist
```

Open http://localhost:5321/. Set `MDBOOK` to an executable path if mdBook is not
on `PATH`. The wrapper runs `mdbook build`, packages tutorial downloads and the
conformance report, and writes the sitemap and plain-text index. It does not
execute any conformance engines.

For automatic guide-only rebuilds while editing:

```sh
mdbook serve website/docs --port 5321
```

This command builds the book alone. Use `build.py` again before checking or
publishing so report assets and downloads are included.

## Editing

- Edit chapters in `content/`; each file has its own `# Title`.
- Edit chapter order and section names in `content/SUMMARY.md`.
- Use relative `.md` links between chapters; mdBook converts them to `.html`.
- Configure mdBook in `book.toml`. Keep its theme unmodified.
- Keep `downloads/` in sync with the tutorials.
- Preserve the existing chapter filenames so published guide URLs stay stable.

`conformance.html` is now the book's conformance guide. Its language anchors
remain available and point to the corresponding sections of the detailed
report. `conformance_page.py` and `leaderboard.py` generate that report from
committed evidence. Raw download paths remain unchanged.

## Browser checks

With the local preview running and Playwright/Chromium installed:

```sh
node website/docs/browser-check.mjs
node website/docs/conformance-check.mjs
```

Set `PLAYWRIGHT_MODULE` to an existing Playwright `index.mjs`, `CHROME_PATH` to
an installed Chrome executable, or `DOCS_URL` to another preview origin.
The book check visits every chapter at desktop and mobile widths, checks
search and code copying, and exercises navigation with and without JavaScript.
Book screenshots are written to `target/site-review/`; the report checker
writes screenshots to `/tmp/conformance-explorer-{desktop,mobile}.png`.

`check.py` verifies generated links, fragments, assets, downloads, and coverage
of all chapters in the mdBook search index.

## Hosting and publishing

- Public output: `website/docs/dist/` (ignored by Git).
- Private S3 prefix: `s3://crabgraph-landing-846199521923/documentation/`.
- CloudFront distribution: `EV4E7ROH7WATO`.
- CloudFront hostname: `d1lmeetba2mo8q.cloudfront.net`.
- DNS: Route 53 A and AAAA aliases for `docs.crabgraph.net`.
- TLS: the existing `crabgraph.net` wildcard ACM certificate.

The site uses `.html` URLs and needs no application server or routing function.
GitHub Actions installs pinned mdBook, then uses
[the deployment script](../scripts/deploy.sh) to build and publish both sites.
The workflow runs no tests or validation; perform the checks locally first.
See [the website README](../README.md) for manual publishing instructions.
