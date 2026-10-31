const { app, BrowserWindow } = require('electron');
const fs = require('node:fs/promises');
const path = require('node:path');

const iconDir = path.join(__dirname, '..', 'assets', 'icons');
const sizes = [16, 24, 32, 48, 64, 128, 256];

app.whenReady().then(async () => {
  const window = new BrowserWindow({ show: false });
  try {
    const source = await fs.readFile(path.join(iconDir, 'radio.svg'), 'utf8');
    await window.loadURL('about:blank');
    const renders = await window.webContents.executeJavaScript(`(async () => {
      const source = ${JSON.stringify(source)};
      const sizes = ${JSON.stringify(sizes)};
      async function draw(size, color, background) {
        const canvas = document.createElement('canvas');
        canvas.width = canvas.height = size;
        const context = canvas.getContext('2d');
        if (background) {
          context.fillStyle = '#10201b';
          context.beginPath();
          context.roundRect(0, 0, size, size, size * 0.19);
          context.fill();
        }
        const svg = source.replace(/fill="#1f1f1f"/i, 'fill="' + color + '"');
        const image = new Image();
        image.src = 'data:image/svg+xml;base64,' + btoa(svg);
        await image.decode();
        const padding = background ? size * 0.12 : size * 0.05;
        context.drawImage(image, padding, padding, size - padding * 2, size - padding * 2);
        return canvas.toDataURL('image/png').split(',')[1];
      }
      return {
        app: await Promise.all(sizes.map((size) => draw(size, '#c7f464', true))),
        tray: await draw(32, '#ffffff', false),
      };
    })()`);
    const pngs = renders.app.map((base64) => Buffer.from(base64, 'base64'));
    const header = Buffer.alloc(6);
    header.writeUInt16LE(1, 2);
    header.writeUInt16LE(sizes.length, 4);
    let offset = 6 + sizes.length * 16;
    const directory = sizes.map((size, index) => {
      const entry = Buffer.alloc(16);
      entry[0] = size === 256 ? 0 : size;
      entry[1] = size === 256 ? 0 : size;
      entry.writeUInt16LE(1, 4);
      entry.writeUInt16LE(32, 6);
      entry.writeUInt32LE(pngs[index].length, 8);
      entry.writeUInt32LE(offset, 12);
      offset += pngs[index].length;
      return entry;
    });
    await fs.writeFile(
      path.join(iconDir, 'radio.ico'),
      Buffer.concat([header, ...directory, ...pngs]),
    );
    await fs.writeFile(path.join(iconDir, 'radio.png'), pngs.at(-1));
    await fs.writeFile(path.join(iconDir, 'tray-mask.png'), Buffer.from(renders.tray, 'base64'));
    process.stdout.write('Radio Replay icons generated.\n');
  } catch (error) {
    process.stderr.write(`${error.stack || error}\n`);
    app.exit(1);
    return;
  } finally {
    window.destroy();
    app.quit();
  }
});
