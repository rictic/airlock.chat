self.addEventListener('install', (event) => {
});

self.addEventListener('fetch', function(event) {
  event.respondWith((async () => {
    try {
      return await fetch(event.request);
    } catch {}
    return new Response("Airlock.chat is offline...", {
      headers: {'Content-Type': 'text/html'}
    });
  })());
});
