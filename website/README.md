# Crabgraph landing page

The landing page is a dependency-free static site. Open `index.html` through a local HTTP server to preview it.

```sh
python3 -m http.server 5320 --directory website
```

Production is the private S3 bucket `crabgraph-landing-846199521923` behind CloudFront distribution `E2LDPO5UT3NIDR`. Upload files with the `personal` AWS profile, then invalidate `/*`.

The deployment replaces the bucket contents. The source of truth for product language is `STYLE.md`, and coverage figures must remain grounded in the repository's benchmark notes under `docs/`.
