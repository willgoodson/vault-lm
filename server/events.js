// Server-Sent Events hub: file changes, index progress, overview status.
const clients = new Set();

export function subscribe(reply) {
  reply.raw.writeHead(200, { 'content-type': 'text/event-stream', 'cache-control': 'no-cache', connection: 'keep-alive' });
  reply.raw.write(': connected\n\n');
  clients.add(reply.raw);
  const ping = setInterval(() => reply.raw.write(': ping\n\n'), 25000);
  reply.raw.on('close', () => { clearInterval(ping); clients.delete(reply.raw); });
}

export function emit(type, data = {}) {
  const msg = `event: ${type}\ndata: ${JSON.stringify(data)}\n\n`;
  for (const c of clients) c.write(msg);
}
