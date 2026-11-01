const { app } = require('electron');
const path = require('node:path');
const { MetadataService } = require('../src/metadata-service.cjs');

app.whenReady().then(async () => {
  const service = new MetadataService();
  try {
    const file = path.join(__dirname, '..', 'assets', 'audio', 'static.mp3');
    const duration = await service.request('duration', file);
    const tags = await service.request('tags', file);
    if (!(duration > 0) || typeof tags.title !== 'string')
      throw new Error('Metadata worker returned invalid results');
    process.stdout.write(`Metadata process passed: duration ${duration.toFixed(2)}s\n`);
  } catch (error) {
    process.stderr.write(`${error.stack || error}\n`);
    app.exit(1);
    return;
  } finally {
    service.close();
    app.quit();
  }
});
