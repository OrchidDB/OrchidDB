#!/usr/bin/env bash
# AWS_PROFILE=personal bash website/scripts/deploy.sh (local)
# GitHub Actions supplies credentials through its environment.
set -euo pipefail
cd "$(dirname "$0")/../.."
python3 website/docs/build.py

bucket=crabgraph-landing-846199521923
# Upload only public landing assets. Never sync the repository or delete docs.
for file in index.html resources.html ecosystem.html community.html blog.html styles.css script.js hero-graph.js favicon.svg og.png og.svg; do
  aws s3 cp "website/$file" "s3://$bucket/$file" \
    --cache-control 'public,max-age=300' --only-show-errors
done
# Publish local technology artwork, excluding source notes.
aws s3 sync website/assets/logos/ "s3://$bucket/assets/logos/" \
  --exclude '*' --include '*.svg' --include '*.png' \
  --cache-control 'public,max-age=300' --only-show-errors
aws s3 sync website/docs/dist/ "s3://$bucket/documentation/" \
  --cache-control 'public,max-age=300' --only-show-errors
aws cloudfront create-invalidation --distribution-id E2LDPO5UT3NIDR \
  --paths '/*' --query 'Invalidation.Id' --output text
aws cloudfront create-invalidation --distribution-id EV4E7ROH7WATO \
  --paths '/*' --query 'Invalidation.Id' --output text
