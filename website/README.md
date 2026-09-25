# Crabgraph project website

The project website at https://crabgraph.net/ uses plain HTML and CSS, a small
mobile-navigation script, and a canvas graph animation. Documentation at
https://docs.crabgraph.net/ uses mdBook's standard theme. Both sites are static.

## Preview

```sh
python3 -m http.server 5320 --directory website
```

For the documentation build and preview, see [docs/README.md](docs/README.md).

## Structure

The page structure follows [OpenLineage](https://openlineage.io/): a compact
header, centered project introduction, About section, usage section,
participation links, and a footer repeating the navigation.

The header and footer use this order: Getting Started, Resources, Ecosystem,
Community, Blog, Docs. Slack and GitHub sit at the right of the desktop header;
Quickstart, Slack, and GitHub sit below the introduction. The footer's second
row contains Mastodon, Twitter, LinkedIn, Slack, and GitHub.

- `index.html`: project introduction, technology logos, data-to-graph mapping diagram,
  application embedding diagram, and participation.
- `resources.html`: guides, examples, and conformance evidence.
- `ecosystem.html`: underlying technologies and query-language guides.
- `community.html`: participation guidance and clearly marked channel placeholders.
- `blog.html`: an empty blog index until there are project posts to publish.
- `assets/logos/`: unmodified technology logos; `README.md` records their official sources.
- `hero-graph.js`: a decorative animated node-and-edge background. It pauses offscreen,
  supports reduced motion, and has a visible pause control. No generated image is loaded.

The header/footer markup is shared by convention across these five plain HTML
pages. Keep navigation changes synchronized. Slack, social, and TSC links point
to named sections on the Community page until actual destinations exist. Do not
invent account URLs, affiliations, or meeting schedules. The existing repository
license is unchanged; the site does not claim the code is open source.

## Verification

Serve both local sites, then run:

```sh
node website/scripts/browser-check.mjs
node website/docs/browser-check.mjs
```

These checks need a local Playwright installation and Chromium. Set
`PLAYWRIGHT_MODULE` to an existing Playwright `index.mjs` and `CHROME_PATH` to
an installed Chrome executable if needed. `SITE_URL` and `DOCS_URL` override
the default preview addresses. Screenshots go to `target/site-review/`.
The project-site check verifies all pages at five viewport widths, navigation
order, internal destinations, placeholders, mobile controls, documentation
search, and navigation without JavaScript.

## Deployment

GitHub Actions installs pinned mdBook 0.5.4, builds, and publishes both sites
when site files or committed comparison evidence change on `main`. It also
supports manual publication. The website workflow runs no tests, conformance
engines, validation jobs, or pull-request checks. Run checks locally before
committing. The separate [release workflow](../.github/workflows/release.yml)
builds CLI archives and smoke-checks them when a version tag is pushed.

The workflow is [website.yml](../.github/workflows/website.yml). AWS access uses
the `personal` account credentials stored as repository Actions secrets:
`CRABGRAPH_AWS_ACCESS_KEY_ID` and `CRABGRAPH_AWS_SECRET_ACCESS_KEY`.

To publish manually, install mdBook first as described in the docs README, then:

```sh
AWS_PROFILE=personal bash website/scripts/deploy.sh
```

The script uploads an explicit list of public project pages and assets, the
SVG/PNG technology logos, and the built documentation. Logo source notes are not
published. It does not replace the entire bucket. Use it rather than
syncing the whole `website/` directory, which includes build sources.

| Site | Private S3 location | CloudFront distribution |
| --- | --- | --- |
| Project website | `crabgraph-landing-846199521923` root | `E2LDPO5UT3NIDR` |
| Docs | Same bucket, `documentation/` prefix | `EV4E7ROH7WATO` |

[STYLE.md](STYLE.md) describes the visual and editorial approach.
