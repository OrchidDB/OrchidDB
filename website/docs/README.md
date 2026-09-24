# Crabgraph documentation

The documentation is a static site at https://docs.crabgraph.net/. All pages are
rendered at build time. Search and copy buttons use local browser JavaScript;
reading and navigation also work with JavaScript disabled. There are no runtime
services or third-party asset requests.

## Build and preview

Python 3.10+ is sufficient; there are no Python package dependencies.

```sh
python3 website/docs/build.py
python3 website/docs/check.py
python3 -m http.server 5321 --directory website/docs/dist
```

Open http://localhost:5321/. Edit the Markdown files in `content/`, assets in
`assets/`, and navigation in `build.py`. The first line of each content file is
its summary and meta description. The small renderer supports paragraphs,
second- and third-level headings, fenced code, tables, bullet lists, notes,
links, bold text, and inline code. Generated `dist/` output is ignored by Git.

Use user-facing task guidance and working examples. Keep development roadmaps
in the repository's engineering documents. Keep downloadable tutorial files in
`downloads/` synchronized with the examples in the guides.

## Browser checks

Install Playwright and its Chromium browser in your development environment,
then run the check against the local preview:

```sh
node website/docs/browser-check.mjs
```

For an existing installation, set `PLAYWRIGHT_MODULE` to its `index.mjs` path.
`CHROME_PATH` optionally selects an installed Chrome executable. Set `DOCS_URL`
to test another origin. The check covers every guide at desktop and mobile
sizes, search, copy buttons, the mobile menu, and navigation without JavaScript.
Screenshots are written to `/tmp/crabgraph-docs-desktop.png` and
`/tmp/crabgraph-docs-mobile.png`.

## Hosting

- Source files: this directory.
- Public output: `website/docs/dist/`.
- S3 prefix: `s3://crabgraph-landing-846199521923/documentation/`.
- CloudFront distribution: `EV4E7ROH7WATO`.
- CloudFront hostname: `d1lmeetba2mo8q.cloudfront.net`.
- DNS: Route 53 A and AAAA aliases for `docs.crabgraph.net`.
- TLS: the existing `crabgraph.net` wildcard ACM certificate.

The distribution reads a private S3 origin through the existing origin access
identity. Pages use `.html` URLs, so no routing functions or application server
are needed. Unknown paths return the static 404 page with HTTP status 404.

## Publishing

The repository workflow `.github/workflows/website.yml` validates pull requests
and publishes both sites on every push to `main`. It also supports manual runs
on `main`. It reads `CRABGRAPH_AWS_ACCESS_KEY_ID` and
`CRABGRAPH_AWS_SECRET_ACCESS_KEY` from GitHub Actions secrets, configured from
the owner's `personal` AWS profile.

For a manual deployment from the repository root:

```sh
AWS_PROFILE=personal bash website/scripts/deploy.sh
```

The script builds and validates the documentation, uploads public assets, and
invalidates both CloudFront distributions. It preserves unrelated bucket keys.
