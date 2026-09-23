# Crabgraph websites

The landing page at https://crabgraph.net/ and documentation at
https://docs.crabgraph.net/ are static sites.

## Preview

```sh
python3 -m http.server 5320 --directory website
```

For the docs build and preview, see [docs/README.md](docs/README.md).

## Deployment

GitHub Actions publishes both sites after each push to `main` and supports a
manual workflow run. Pull requests validate the static build without deployment.
The workflow is [website.yml](../.github/workflows/website.yml). AWS access uses
the `personal` account credentials stored as repository Actions secrets:
`CRABGRAPH_AWS_ACCESS_KEY_ID` and `CRABGRAPH_AWS_SECRET_ACCESS_KEY`.

To publish manually:

```sh
AWS_PROFILE=personal bash website/scripts/deploy.sh
```

The script uploads only public assets and invalidates both distributions. It
does not replace the entire bucket. Use this script rather than syncing the
whole `website/` directory, which includes build sources and documentation.

| Site | Private S3 location | CloudFront distribution |
| --- | --- | --- |
| Landing | `crabgraph-landing-846199521923` root | `E2LDPO5UT3NIDR` |
| Docs | Same bucket, `documentation/` prefix | `EV4E7ROH7WATO` |

`STYLE.md` describes the site's visual and editorial vocabulary. Documentation
focuses on usage, working examples, and API contracts.
