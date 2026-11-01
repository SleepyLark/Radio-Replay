const { parentPort } = process;

parentPort.on('message', async ({ data: { id, kind, file } }) => {
  try {
    const { parseFile } = await import('music-metadata');
    const { common, format } = await parseFile(file, {
      skipCovers: true,
      duration: kind === 'duration',
    });
    const value =
      kind === 'tags'
        ? { artist: common.artist || '', title: common.title || '', album: common.album || '' }
        : format.duration || 0;
    parentPort.postMessage({ id, value });
  } catch (error) {
    parentPort.postMessage({ id, error: error.message });
  }
});
