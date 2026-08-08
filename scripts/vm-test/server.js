// Minimal static file server for the VM-test drop folder. No deps.
// Usage: node server.js <port> <dir>
const http = require('http');
const fs = require('fs');
const path = require('path');

const port = parseInt(process.argv[2], 10) || 8712;
const root = path.resolve(process.argv[3] || '.');

const server = http.createServer((req, res) => {
  const name = decodeURIComponent(req.url.split('?')[0].replace(/^\/+/, ''));
  if (name === '') {
    const files = fs.readdirSync(root).filter(f => fs.statSync(path.join(root, f)).isFile());
    const body = files.map(f => `<li><a href="/${encodeURIComponent(f)}">${f}</a></li>`).join('\n');
    res.writeHead(200, { 'Content-Type': 'text/html' });
    res.end(`<h1>DML VM test drop</h1><ul>${body}</ul>`);
    return;
  }
  const file = path.join(root, name);
  // refuse anything that resolves outside the drop folder
  if (!file.startsWith(root + path.sep) || path.basename(file) !== name) {
    res.writeHead(400); res.end('bad path'); return;
  }
  fs.stat(file, (err, st) => {
    if (err || !st.isFile()) { res.writeHead(404); res.end('not found'); return; }
    res.writeHead(200, { 'Content-Type': 'application/octet-stream', 'Content-Length': st.size });
    fs.createReadStream(file).pipe(res);
    console.log(`served ${name} (${st.size} bytes) to ${req.socket.remoteAddress}`);
  });
});

server.listen(port, () => console.log(`serving ${root} on port ${port} - Ctrl+C to stop`));
