// CloudFront viewer-request function. Keep paths and search terms on old links.
function handler(event) {
  var request = event.request;
  var host = request.headers.host.value.toLowerCase();
  var target = host.indexOf('docs.') === 0 ? 'docs.orchiddb.com' : 'orchiddb.com';
  if (host === target) return request;
  var params = [];
  var query = request.querystring;
  for (var key in query) {
    var values = query[key].multiValue || [query[key]];
    for (var i = 0; i < values.length; i++) {
      params.push(key + '=' + values[i].value);
    }
  }
  return {
    statusCode: 301,
    statusDescription: 'Moved Permanently',
    headers: {
      location: { value: 'https://' + target + request.uri + (params.length ? '?' + params.join('&') : '') },
      'cache-control': { value: 'public, max-age=300' }
    }
  };
}
