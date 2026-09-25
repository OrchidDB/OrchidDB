#!/usr/bin/env bash
# AWS_PROFILE=personal bash website/docs/deploy.sh
set -euo pipefail
cd "$(dirname "$0")/../.."
python3 website/docs/build.py
# Docs own only this prefix and distribution. Landing/installer deploy separately.
aws s3 sync website/docs/dist/ s3://orchiddb-landing-846199521923/documentation/ \
  --cache-control 'public,max-age=300' --only-show-errors
aws cloudfront create-invalidation --distribution-id EV4E7ROH7WATO \
  --paths '/*' --query 'Invalidation.Id' --output text
