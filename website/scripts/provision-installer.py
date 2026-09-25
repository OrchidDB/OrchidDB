#!/usr/bin/env python3
"""Provision the installer CDN and DNS using existing OrchidDB S3 origin/TLS."""
import json
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]

def aws(*args):
    return json.loads(subprocess.check_output(['aws', *args, '--output', 'json']))

existing = aws('cloudfront', 'list-distributions').get('DistributionList', {}).get('Items', [])
match = next((d for d in existing if 'install.orchiddb.com' in d.get('Aliases', {}).get('Items', [])), None)
if match is None:
    config = aws('cloudfront', 'get-distribution-config', '--id', 'E2LDPO5UT3NIDR')['DistributionConfig']
    config['CallerReference'] = f'orchiddb-installer-{int(time.time())}'
    config['Aliases'] = {'Quantity': 1, 'Items': ['install.orchiddb.com']}
    config['DefaultRootObject'] = 'install.sh'
    config['Comment'] = 'OrchidDB release installer'
    config['Origins']['Items'][0]['OriginPath'] = '/install'
    config['DefaultCacheBehavior']['FunctionAssociations'] = {'Quantity': 0}
    match = aws('cloudfront', 'create-distribution', '--distribution-config', json.dumps(config))['Distribution']
records = {'Comment': 'OrchidDB installer CDN', 'Changes': [
    {'Action': 'UPSERT', 'ResourceRecordSet': {'Name': 'install.orchiddb.com', 'Type': kind,
     'AliasTarget': {'HostedZoneId': 'Z2FDTNDATAQYW2', 'DNSName': match['DomainName'], 'EvaluateTargetHealth': False}}}
    for kind in ['A', 'AAAA']]}
aws('route53', 'change-resource-record-sets', '--hosted-zone-id', 'Z094613320U7C0GG8MQQH', '--change-batch', json.dumps(records))
result = {'distribution_id': match['Id'], 'domain': match['DomainName'], 'hostname': 'install.orchiddb.com', 's3_prefix': 'install/'}
(ROOT / 'website/install/infrastructure.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps(result))
