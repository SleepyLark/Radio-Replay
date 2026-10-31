const $ = (q) => document.querySelector(q);
// Only `audio` is audible during normal playback. `spare` is used during crossfades.
let audio = $('#audio');
let spare = $('#audioNext');
const staticAudio = $('#staticAudio');

const get = (k, fallback) => {
  // Read settings from the pre-JSON version once, then persist them in stations.json.
  try {
    return JSON.parse(localStorage.getItem(k)) ?? fallback;
  } catch {
    return fallback;
  }
};
let root = localStorage.getItem('radio-library') || '';
let custom = get('custom-stations', []);
let groups = get('station-groups', []);
let logos = get('station-logos', {});
let hidden = [];
let settings = {
  accent: '#c7f464',
  staticPath: 'bundled',
  crossfade: 0,
  bounceSpeed: 40,
  matchBannerAccent: false,
};
let bundledStaticPath = '';

let stations = [];
let selected = null;
let groupId = 'all';
let sidecar = null;
let displayedTrackPath = '';
let token = 0;
let importPaths = [];
let currentIndex = 0;
let currentClock = null;
let fading = false;
let fadeTimer = null;
let stepTimer = null;
let stepTarget = null;
let lastWheelStep = 0;

const clocks = new Map();
const durations = new Map();
const unavailable = new Set();

function updateUnavailableNotice() {
  $('#playbackNotice').textContent = unavailable.size
    ? `${unavailable.size} file${unavailable.size === 1 ? '' : 's'} unavailable. Manage stations to update paths.`
    : '';
}

function validStation(station) {
  return (
    station &&
    typeof station.key === 'string' &&
    typeof station.station === 'string' &&
    Array.isArray(station.tracks) &&
    station.tracks.every(
      (track) => track && typeof track.path === 'string' && typeof track.name === 'string',
    )
  );
}

function sanitizeConfig(config) {
  if (!config || typeof config !== 'object' || Array.isArray(config))
    throw new Error('Invalid stations.json structure');
  root = typeof config.root === 'string' ? config.root : '';
  custom = Array.isArray(config.custom)
    ? config.custom.filter(validStation).map((station) => ({
        ...station,
        bannerPath: typeof station.bannerPath === 'string' ? station.bannerPath : '',
        bannerColor: /^#[0-9a-f]{6}$/i.test(station.bannerColor) ? station.bannerColor : '',
        bannerAccent: /^#[0-9a-f]{6}$/i.test(station.bannerAccent) ? station.bannerAccent : '',
        spinLogo: station.spinLogo === true,
      }))
    : [];
  groups = Array.isArray(config.groups)
    ? config.groups
        .filter(
          (g) =>
            g && typeof g.id === 'string' && typeof g.name === 'string' && Array.isArray(g.members),
        )
        .map((g) => ({ ...g, members: g.members.filter((m) => typeof m === 'string') }))
    : [];
  logos =
    config.logos && typeof config.logos === 'object' && !Array.isArray(config.logos)
      ? Object.fromEntries(
          Object.entries(config.logos).filter(([, value]) => typeof value === 'string'),
        )
      : {};
  hidden = Array.isArray(config.hidden)
    ? config.hidden.filter((key) => typeof key === 'string')
    : [];
  const loaded =
    config.settings && typeof config.settings === 'object' && !Array.isArray(config.settings)
      ? config.settings
      : {};
  settings = {
    accent: typeof loaded.accent === 'string' ? loaded.accent : settings.accent,
    staticPath: typeof loaded.staticPath === 'string' ? loaded.staticPath : settings.staticPath,
    crossfade: Number.isFinite(loaded.crossfade) ? loaded.crossfade : settings.crossfade,
    bounceSpeed: Number.isFinite(loaded.bounceSpeed) ? loaded.bounceSpeed : settings.bounceSpeed,
    matchBannerAccent: loaded.matchBannerAccent === true,
  };
}

function invalidateStationClock(stationKey) {
  for (const key of clocks.keys()) if (key.endsWith(':' + stationKey)) clocks.delete(key);
}

function updateTitleBounce() {
  const viewport = $('#titleViewport');
  const title = $('#trackTitle');
  const overflow = Math.ceil(title.scrollWidth - viewport.clientWidth);
  title.classList.toggle('bouncing', overflow > 0);
  if (overflow > 0) {
    title.style.setProperty('--bounce-distance', `${overflow}px`);
    title.style.setProperty(
      '--bounce-duration',
      `${overflow / (Number(settings.bounceSpeed) || 40)}s`,
    );
  }
}

// LRC updates the title during playback, so observe text and available width.
new MutationObserver(updateTitleBounce).observe($('#trackTitle'), {
  childList: true,
  characterData: true,
  subtree: true,
});
new ResizeObserver(updateTitleBounce).observe($('#titleViewport'));

// All user edits go through the main process so the config file remains outside the bundle.
const save = () =>
  window.radio
    .saveConfig({ version: 2, root, custom, groups, logos, hidden, settings })
    .catch((error) => {
      $('#libraryPath').textContent = `Settings could not be saved: ${error.message}`;
    });
const esc = (s) => {
  const e = document.createElement('i');
  e.textContent = String(s ?? '');
  return e.innerHTML.replace(/"/g, '&quot;').replace(/'/g, '&#39;');
};
const url = (filePath) => {
  const normalized = filePath.replace(/\\/g, '/');
  if (normalized.startsWith('//')) {
    const [host, ...parts] = normalized.slice(2).split('/');
    return `file://${host}/${parts.map(encodeURIComponent).join('/')}`;
  }
  if (/^[a-z]:\//i.test(normalized))
    return `file:///${normalized.slice(0, 2)}/${normalized.slice(3).split('/').map(encodeURIComponent).join('/')}`;
  if (normalized.startsWith('/'))
    return `file://${normalized.split('/').map(encodeURIComponent).join('/')}`;
  throw new Error('Expected an absolute file path');
};
const fmt = (s) =>
  Number.isFinite(s)
    ? `${String(Math.floor(s / 60)).padStart(2, '0')}:${String(Math.floor(s % 60)).padStart(2, '0')}`
    : '00:00';
const shuffle = (a) => {
  const b = [...a];
  for (let i = b.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [b[i], b[j]] = [b[j], b[i]];
  }
  return b;
};
const memberBelongsToStation = (member, station) =>
  member === station.key ||
  station.tracks.some((track) => member === station.key + '|' + track.path);
const visible = () => {
  if (groupId === 'all') return stations;
  const members = new Set(groups.find((g) => g.id === groupId)?.members || []);
  return stations.filter(
    (station) =>
      members.has(station.key) ||
      station.tracks.some((track) => members.has(station.key + '|' + track.path)),
  );
};
const fadeFor = (s) =>
  s.mode === 'tracks'
    ? Math.max(0, Math.min(30, Number(s.crossfade ?? settings.crossfade) || 0))
    : 0;

function applySettings() {
  applyAccent(selected);
  $('#accentColor').value = settings.accent || '#c7f464';
  $('#matchBannerAccent').checked = settings.matchBannerAccent;
  $('#globalFade').value = settings.crossfade ?? 0;
  $('#bounceSpeed').value = settings.bounceSpeed ?? 40;
  updateTitleBounce();
  $('#staticName').textContent =
    settings.staticPath === 'bundled'
      ? 'Bundled static.mp3'
      : settings.staticPath === 'off'
        ? 'Disabled'
        : settings.staticPath.split(/[\\/]/).at(-1);
}

function accentFor(station) {
  const base = /^#[0-9a-f]{6}$/i.test(settings.accent) ? settings.accent : '#c7f464';
  if (!settings.matchBannerAccent || !station) return base;
  const stationColor = station.bannerPath ? station.bannerAccent : station.bannerColor;
  return /^#[0-9a-f]{6}$/i.test(stationColor) ? stationColor : base;
}

function applyAccent(station) {
  const accent = accentFor(station);
  document.documentElement.style.setProperty('--green', accent);
  window.radio.setAppAccent(accent);
}

function applyStationVisuals(station) {
  const dial = $('.dial');
  const image = $('#dialBanner');
  const color = /^#[0-9a-f]{6}$/i.test(station.bannerColor) ? station.bannerColor : '';
  dial.style.backgroundColor = color || '#0b1513';
  image.hidden = !station.bannerPath;
  if (station.bannerPath) image.src = url(station.bannerPath);
  $('#logo').classList.toggle('spin-enabled', station.spinLogo === true);
  updateLogoMotion();
  applyAccent(station);
}

function updateLogoMotion() {
  $('#logo').classList.toggle('is-playing', !!selected && !audio.paused);
}

function stopStatic() {
  staticAudio.pause();
  try {
    staticAudio.currentTime = 0;
  } catch {
    /* No loaded source to rewind. */
  }
}

function startStatic() {
  const path =
    settings.staticPath === 'bundled'
      ? bundledStaticPath
      : settings.staticPath === 'off'
        ? ''
        : settings.staticPath;
  if (path) {
    try {
      const source = url(path);
      if (staticAudio.src !== source) staticAudio.src = source;
      staticAudio.currentTime = 0;
      staticAudio.volume = Math.min(0.5, Number($('#volume').value) / 200);
      staticAudio.play().catch(() => {});
    } catch {
      /* Static is optional; audio playback can continue. */
    }
  }
}

function stopFade() {
  if (fadeTimer) clearInterval(fadeTimer);
  fadeTimer = null;
  fading = false;
  spare.pause();
  spare.removeAttribute('src');
  spare.load();
  audio.volume = Number($('#volume').value) / 100;
}

function notice(message) {
  $('#trackTitle').textContent = message;
  $('#metadata').textContent = '';
}

function waitForMetadata(element, current) {
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => finish(new Error('Audio metadata timed out')), 10000);
    const finish = (error) => {
      clearTimeout(timeout);
      element.removeEventListener('loadedmetadata', loaded);
      element.removeEventListener('error', failed);
      if (error) reject(error);
      else resolve();
    };
    const loaded = () => finish();
    const failed = () => finish(new Error('Audio file could not be loaded'));
    if (current !== token) return finish(new Error('Tuning changed'));
    element.addEventListener('loadedmetadata', loaded, { once: true });
    element.addEventListener('error', failed, { once: true });
    if (element.readyState >= HTMLMediaElement.HAVE_METADATA) finish();
  });
}

function sidebarStationMarkup(station) {
  const active = selected?.key === station.key ? 'active' : '';
  const description = station.mode === 'longset' ? 'broadcast' : `${station.tracks.length} tracks`;

  return `
    <button class="station ${active}" data-key="${esc(station.key)}">
      <span>${esc(station.station)}</span>
      <small>${description}</small>
    </button>
  `;
}

function homeStationMarkup(station) {
  const artwork = logos[station.key]
    ? `<img src="${esc(url(logos[station.key]))}" alt="" loading="lazy" decoding="async">`
    : '<span class="home-icon">◉</span>';
  const description =
    station.mode === 'longset' ? 'Continuous broadcast' : `${station.tracks.length} tracks`;

  return `
    <button class="home-station" data-key="${esc(station.key)}">
      ${artwork}
      <span>
        ${station.game ? `<small>${esc(station.game)}</small>` : ''}
        <strong>${esc(station.station)}</strong>
        <em>${description}</em>
      </span>
    </button>
  `;
}

function render() {
  // The sidebar and the home screen are two views of the same filtered station list.
  $('#libraryPath').textContent = `${stations.length} station${stations.length === 1 ? '' : 's'}`;
  $('#groupSelect').innerHTML =
    `<option value="all">All stations</option>${groups.map((g) => `<option value="${esc(g.id)}">${esc(g.name)}</option>`).join('')}`;
  if (!groups.some((g) => g.id === groupId)) groupId = 'all';
  $('#groupSelect').value = groupId;
  $('#editGroup').disabled = groupId === 'all';
  const sections = new Map();
  const shown = visible();
  for (const s of shown) {
    const heading = groupId === 'all' ? s.game || '' : 'Stations';
    if (!sections.has(heading)) sections.set(heading, []);
    sections.get(heading).push(s);
  }
  $('#stationList').innerHTML =
    [...sections]
      .map(
        ([heading, list]) =>
          `<section class="game">${heading ? `<h2>${esc(heading)}</h2>` : ''}${list.map(sidebarStationMarkup).join('')}</section>`,
      )
      .join('') || '<div class="empty">No stations in this group.</div>';
  $('#homeSummary').textContent =
    `${shown.length} station${shown.length === 1 ? '' : 's'} available`;
  $('#homeStations').innerHTML =
    shown.map(homeStationMarkup).join('') ||
    '<p class="empty">No stations yet. Create one to get started.</p>';
  updateActiveStation();
}

function updateActiveStation() {
  for (const button of document.querySelectorAll('.station, .home-station')) {
    const active = button.dataset.key === selected?.key;
    button.classList.toggle('active', active);
    if (active) button.setAttribute('aria-current', 'true');
    else button.removeAttribute('aria-current');
  }
}

for (const list of [$('#stationList'), $('#homeStations')]) {
  list.addEventListener('click', (event) => {
    const button = event.target.closest('[data-key]');
    if (button && list.contains(button)) tune(stations.find((s) => s.key === button.dataset.key));
  });
}

function parse(raw) {
  // Any audio file may have an adjacent LRC, text, or JSON sidecar.
  if (!raw) return null;
  if (raw.extension === '.json') {
    try {
      const data = JSON.parse(raw.text);
      if (!data || typeof data !== 'object' || Array.isArray(data)) return null;
      return {
        title: typeof data.title === 'string' ? data.title : '',
        artist: typeof data.artist === 'string' ? data.artist : '',
        album: typeof data.album === 'string' ? data.album : '',
        text: typeof data.text === 'string' ? data.text : '',
        timed: Array.isArray(data.timed)
          ? data.timed
              .filter((x) => x && Number.isFinite(x.at) && typeof x.text === 'string')
              .sort((a, b) => a.at - b.at)
          : [],
      };
    } catch {
      return { text: raw.text };
    }
  }
  const timed = [],
    plain = [];
  for (const line of raw.text.replace(/^\uFEFF/, '').split(/\r?\n/)) {
    const stamps = [...line.matchAll(/\[(\d+):(\d+(?:\.\d+)?)\]/g)];
    if (stamps.length && line.startsWith(stamps[0][0])) {
      const lyric = line.replace(/^(?:\[\d+:\d+(?:\.\d+)?\])+\s*/, '');
      for (const m of stamps) timed.push({ at: +m[1] * 60 + +m[2], text: lyric });
    } else if (line.trim() && !/^\[[a-z]+:/i.test(line)) plain.push(line.trim());
  }
  return { timed: timed.sort((a, b) => a.at - b.at), text: plain.join(' • ') };
}

function metadata() {
  if (!sidecar) return;

  let currentLine;
  if (sidecar.timed?.length) {
    let low = 0,
      high = sidecar.timed.length;
    while (low < high) {
      const middle = (low + high) >> 1;
      if (sidecar.timed[middle].at <= audio.currentTime) low = middle + 1;
      else high = middle;
    }
    currentLine = sidecar.timed[low - 1]?.text;
  }
  const singleContinuous = selected?.mode === 'longset' && selected.tracks.length === 1;

  if (singleContinuous && sidecar.timed?.length) {
    // For a one-file broadcast, LRC timestamps identify the current song.
    if (currentLine && $('#trackTitle').textContent !== currentLine)
      $('#trackTitle').textContent = currentLine;
    $('#trackAlbum').textContent = '';
    $('#metadata').textContent = '';
    return;
  }

  if (singleContinuous) {
    $('#trackAlbum').textContent = '';
    $('#metadata').textContent = '';
    return;
  }

  const next = currentLine || sidecar.text || '';
  if ($('#metadata').textContent !== next) $('#metadata').textContent = next;
}

async function showTrackDetails(track, station, currentToken) {
  // Both tuning and crossfade completion use this path so LRC timing follows the active MP3.
  displayedTrackPath = track.path;
  sidecar = null;
  $('#trackTitle').textContent = track.name;
  $('#trackAlbum').textContent = '';
  $('#metadata').textContent = '';

  const singleContinuous = station.mode === 'longset' && station.tracks.length === 1;
  const [sidecarFile, tags] = await Promise.all([
    window.radio.readSidecar(track.path),
    singleContinuous ? null : window.radio.readTags(track.path),
  ]);
  if (currentToken !== token || displayedTrackPath !== track.path) return;

  sidecar = parse(sidecarFile);
  if (singleContinuous) {
    if (sidecar?.title) $('#trackTitle').textContent = sidecar.title;
  } else {
    const title = tags?.title || sidecar?.title || track.name;
    const artist = tags?.artist || sidecar?.artist;
    $('#trackTitle').textContent = artist ? `${artist} - ${title}` : title;
    $('#trackAlbum').textContent = tags?.album || sidecar?.album || '';
  }
  metadata();
}

function probe(path) {
  // Metadata-only audio elements provide durations without buffering every track in RAM.
  if (durations.has(path)) return Promise.resolve(durations.get(path));
  return new Promise((resolve) => {
    const a = new Audio();
    let done = false;
    const timeout = setTimeout(() => finish(0), 10000);
    const finish = (n) => {
      if (done) return;
      done = true;
      clearTimeout(timeout);
      a.removeAttribute('src');
      a.load();
      if (n > 0) durations.set(path, n);
      resolve(n);
    };
    a.onloadedmetadata = () => finish(Number.isFinite(a.duration) ? a.duration : 0);
    a.onerror = () => finish(0);
    a.preload = 'metadata';
    a.src = url(path);
  });
}

async function probeLengths(order, current) {
  const lengths = new Array(order.length);
  let next = 0;
  await Promise.all(
    Array.from({ length: Math.min(6, order.length) }, async () => {
      while (next < order.length && current === token) {
        const index = next++;
        lengths[index] = await probe(order[index].path);
      }
    }),
  );
  if (current !== token) throw new Error('Tuning changed');
  return lengths;
}

async function clockFor(s, current) {
  // A clock is created once per station/group in this session. No background audio runs.
  // `steps` are virtual play durations: each ends when the next track's overlap starts.
  const clockKey = `${groupId}:${s.key}`;
  if (clocks.has(clockKey)) return clocks.get(clockKey);
  const included = new Set(groups.find((g) => g.id === groupId)?.members || []);
  const selectedTracks =
    groupId !== 'all' && !included.has(s.key)
      ? s.tracks.filter((t) => included.has(s.key + '|' + t.path))
      : s.tracks;
  const order = s.mode === 'longset' ? [...selectedTracks] : shuffle(selectedTracks);
  const lengths = await probeLengths(order, current);
  const fade =
    order.length > 1 && lengths.every((n) => n > 0)
      ? Math.min(fadeFor(s), ...lengths.map((n) => n / 2))
      : 0;
  const steps = lengths.map((n) => n - fade);
  // Each track's virtual timeslot ends when its overlap with the next track begins.
  const total = steps.reduce((sum, n) => sum + n, 0);
  const clock = {
    order,
    lengths,
    steps,
    fade,
    total,
    origin: performance.now(),
    offset: Math.random() * total,
    usesGlobalFade: s.crossfade == null,
  };
  clocks.set(clockKey, clock);
  return clock;
}

function position(clock) {
  // Compute where a station would be now, even if it has not been played for a while.
  // `origin` and `offset` are session-only; restarting picks a new random point.
  if (!clock.total) return { index: 0, seconds: 0 };
  let elapsed = (clock.offset + (performance.now() - clock.origin) / 1000) % clock.total;
  for (let i = 0; i < clock.steps.length; i++) {
    if (elapsed < clock.steps[i] || i === clock.steps.length - 1)
      return { index: i, seconds: elapsed };
    elapsed -= clock.steps[i];
  }
}

function startFade() {
  // Mix the outgoing and incoming elements, then swap their roles for the next track.
  if (fading || !currentClock?.fade || !selected || audio.paused) return;
  fading = true;
  const outgoing = audio,
    incoming = spare,
    fade = currentClock.fade;
  const nextIndex = (currentIndex + 1) % currentClock.order.length;
  incoming.onloadedmetadata = null;
  const nextSource = url(currentClock.order[nextIndex].path);
  if (incoming.src !== nextSource) incoming.src = nextSource;
  incoming.currentTime = 0;
  incoming.volume = 0;
  incoming
    .play()
    .then(() => {
      if (!fading || outgoing !== audio) return;
      const started = performance.now();
      fadeTimer = setInterval(() => {
        const progress = Math.min(1, (performance.now() - started) / (fade * 1000));
        const level = Number($('#volume').value) / 100;
        outgoing.volume = level * Math.cos((progress * Math.PI) / 2);
        incoming.volume = level * Math.sin((progress * Math.PI) / 2);
        if (progress >= 1) {
          clearInterval(fadeTimer);
          fadeTimer = null;
          fading = false;
          outgoing.pause();
          outgoing.removeAttribute('src');
          outgoing.load();
          outgoing.volume = level;
          audio = incoming;
          spare = outgoing;
          currentIndex = nextIndex;
          updateLogoMotion();
          showTrackDetails(currentClock.order[nextIndex], selected, token).catch(() => {});
        }
      }, 40);
    })
    .catch(() => {
      fading = false;
      incoming.removeAttribute('src');
      incoming.load();
    });
}

async function advance() {
  if (!selected || !currentClock?.order.length) return;
  const current = ++token;
  stopStatic();
  const clock = currentClock;
  const station = selected;
  const order = clock.order;
  audio.pause();
  for (let attempted = 0; attempted < order.length; attempted++) {
    const nextIndex = (currentIndex + 1) % order.length;
    currentIndex = nextIndex;
    const track = order[nextIndex];
    if (unavailable.has(track.path)) continue;
    try {
      audio.src = url(track.path);
      await waitForMetadata(audio, current);
      if (current !== token) return;
      audio.currentTime = 0;
      await audio.play();
      if (current !== token) return;
      clock.origin = performance.now();
      clock.offset = clock.steps.slice(0, nextIndex).reduce((sum, step) => sum + step, 0);
      showTrackDetails(track, station, current).catch(() => {});
      return;
    } catch {
      if (current !== token) return;
      unavailable.add(track.path);
      updateUnavailableNotice();
    }
  }
  if (current === token) notice('No playable tracks in this station');
}

async function tune(s) {
  // The token invalidates pending metadata/sidecar work when tuning changes quickly.
  if (!s) return;
  $('#settingsView').hidden = true;
  $('#homeView').hidden = true;
  $('#playerView').hidden = false;
  const current = ++token;
  clearTimeout(stepTimer);
  stepTimer = null;
  stepTarget = null;
  stopFade();
  audio.pause();
  currentClock = null;
  sidecar = null;
  displayedTrackPath = '';
  selected = s;
  startStatic();
  $('#frequency').textContent = String(
    visible().findIndex((item) => item.key === s.key) + 1,
  ).padStart(2, '0');
  $('#stationName').textContent = s.station;
  $('#stationSub').textContent =
    s.mode === 'longset' ? 'CONTINUOUS BROADCAST' : `${s.tracks.length} TRACKS • SHUFFLE PROGRAM`;
  $('#logo').hidden = !logos[s.key];
  if (logos[s.key]) $('#logo').src = url(logos[s.key]);
  applyStationVisuals(s);
  $('#trackTitle').textContent = 'Tuning…';
  $('#trackAlbum').textContent = '';
  $('#metadata').textContent = '';
  $('#time').textContent = '00:00 / 00:00';
  updateActiveStation();
  try {
    const clock = await clockFor(s, current);
    if (current !== token) return;
    currentClock = clock;
    const p = position(clock),
      track = clock.order[p.index];
    if (!track) {
      notice('No playable tracks in this station');
      return;
    }
    currentIndex = p.index;
    if (unavailable.has(track.path)) {
      await advance();
      return;
    }
    audio.src = url(track.path);
    await waitForMetadata(audio, current);
    if (current !== token) return;
    audio.currentTime = Number.isFinite(audio.duration)
      ? Math.min(p.seconds, Math.max(0, audio.duration - 0.1))
      : Math.max(0, p.seconds);
    showTrackDetails(track, s, current).catch(() => {});
    await audio.play();
  } catch (error) {
    if (current === token) {
      const path = currentClock?.order[currentIndex]?.path;
      if (path) {
        unavailable.add(path);
        updateUnavailableNotice();
        await advance();
      } else notice(error.message || 'Could not play this station');
    }
  } finally {
    if (current === token) stopStatic();
  }
}

function step(n) {
  const list = visible();
  if (!list.length) return;
  const index = list.findIndex((s) => s.key === (stepTarget?.key || selected?.key));
  stepTarget = list[(Math.max(0, index) + n + list.length) % list.length];
  $('#frequency').textContent = String(list.indexOf(stepTarget) + 1).padStart(2, '0');
  $('#stationName').textContent = stepTarget.station;
  clearTimeout(stepTimer);
  stepTimer = setTimeout(() => tune(stepTarget), 180);
}

async function scan(path) {
  // Legacy library roots still load automatically, but new stations use the creator.
  $('#libraryPath').textContent = 'Scanning library…';
  try {
    const found = await window.radio.scanLibrary(path);
    stations = [
      ...found
        .filter((s) => !hidden.includes(s.key) && !custom.some((c) => c.key === s.key))
        .map((s) => ({ ...s, mode: s.tracks.length === 1 ? 'longset' : 'tracks' })),
      ...custom,
    ];
    root = path;
    $('#libraryPath').textContent = `${stations.length} stations`;
    render();
  } catch (e) {
    stations = [...custom];
    render();
    $('#libraryPath').textContent = `Library unavailable: ${e.message}. Custom stations are shown.`;
  }
}

function editor(title, html, commit) {
  const dialog = $('#editor');
  $('#editorTitle').textContent = title;
  $('#editorBody').innerHTML = html;
  $('#editorSave').textContent = 'Save';
  dialog.showModal();
  $('#editorCancel').onclick = () => dialog.close('cancel');
  $('#editorSave').onclick = () => {
    if (commit()) dialog.close('save');
  };
  dialog.onclick = null;
  dialog.querySelector('form').onsubmit = (event) => {
    event.preventDefault();
    $('#editorSave').click();
  };
}

function stationFormMarkup(station, logoPath, bannerPath) {
  return `
    <label>Station name
      <input id="newName" value="${esc(station?.station || '')}" placeholder="My radio station">
    </label>
    <div class="file-actions">
      <button type="button" id="browseFiles">Add files</button>
      <button type="button" id="browseFolder">Add folder</button>
    </div>
    <label>Audio arrangement
      <select id="newMode">
        <option value="tracks">Individual tracks in one station</option>
        <option value="longset">Continuous longset (one station per file on creation)</option>
      </select>
    </label>
    <p id="fileCount"></p>
    <div id="fileList" class="member-list"></div>
    <div class="editor-section">
      <strong>Station logo</strong>
      <p>Set a PNG, JPG, WebP, or animated GIF for the station.</p> 
      <div id="logoName">${esc(logoPath || 'None')}</div>
      <button type="button" id="browseLogo">Choose logo</button>
      <button type="button" id="removeLogo">Remove logo</button>
      <label class="editor-check"><input id="spinLogo" type="checkbox" ${station?.spinLogo ? 'checked' : ''}> Spin logo while playing</label>
    </div>
    <div class="editor-section">
      <strong>Station banner</strong>
      <p>Use an image or color across the radio display behind the station name.</p>
      <div id="bannerName">${esc(bannerPath || 'No banner image')}</div>
      <button type="button" id="browseBanner">Choose banner image</button>
      <button type="button" id="removeBanner">Remove image</button>
      <label class="editor-check"><input id="useBannerColor" type="checkbox" ${station?.bannerColor ? 'checked' : ''}> Use a banner color</label>
      <label class="inline-color">Banner color <input id="bannerColor" type="color" value="${esc(station?.bannerColor || '#244b42')}"></label>
    </div>
    
    <div id="crossfadeField" hidden class="editor-section">
      <label class="editor-check"><input id="useGlobalFade" type="checkbox" ${station?.crossfade == null ? 'checked' : ''}> Use global Crossfade</label>
      <label id="stationFadeLabel" class="inline-number">Station crossfade <input id="stationFade" type="number" min="0" max="30" step="0.5"
        value="${station?.crossfade == null ? '0' : Math.min(30, station.crossfade)}"> seconds</label>
    </div>
    ${station ? '<button type="button" id="deleteStation" class="danger">Delete station</button>' : ''}
    <p id="editorError" class="error"></p>
  `;
}

function stationEditor(station = null, paths = []) {
  // Editing a scanned station turns it into a saved override and hides the scanned original.
  importPaths = station ? station.tracks.map((t) => t.path) : paths;
  let logoPath = station ? logos[station.key] || '' : '';
  let bannerPath = station?.bannerPath || '';
  let bannerAccent = station?.bannerAccent || '';
  const basename = (p) =>
    p
      .split(/[\\/]/)
      .at(-1)
      .replace(/\.[^.]+$/, '');
  const listFiles = () => {
    $('#fileCount').textContent = `${importPaths.length} audio files`;
    $('#fileList').innerHTML = importPaths
      .map(
        (p, i) =>
          `<div class="file-row"><span title="${esc(p)}">${esc(basename(p))}</span><button type="button" data-remove="${i}">Remove</button></div>`,
      )
      .join('');
    document.querySelectorAll('[data-remove]').forEach(
      (b) =>
        (b.onclick = () => {
          importPaths.splice(Number(b.dataset.remove), 1);
          listFiles();
        }),
    );
  };
  editor(
    station ? 'Edit station' : 'Create station',
    stationFormMarkup(station, logoPath, bannerPath),
    () => {
      const name = $('#newName').value.trim(),
        mode = $('#newMode').value;
      if (!name || !importPaths.length) {
        $('#editorError').textContent = 'Enter a name and add at least one audio file.';
        return false;
      }
      const customFade = mode === 'tracks' && !$('#useGlobalFade').checked;
      const override = customFade ? $('#stationFade').value.trim() : '';
      if (
        customFade &&
        (!override ||
          !Number.isFinite(Number(override)) ||
          Number(override) < 0 ||
          Number(override) > 30)
      ) {
        $('#editorError').textContent = 'Crossfade must be between 0 and 30 seconds.';
        return false;
      }
      if (station) {
        const wasCustom = custom.some((s) => s.key === station.key);
        if (!wasCustom) hidden.push(station.key);
        custom = custom.filter((s) => s.key !== station.key);
        const remaining = new Set(importPaths);
        for (const group of groups)
          group.members = group.members.filter(
            (member) =>
              !station.tracks.some(
                (track) => member === station.key + '|' + track.path && !remaining.has(track.path),
              ),
          );
      }
      const entries =
        !station && mode === 'longset'
          ? importPaths.map((p) => ({
              name: importPaths.length === 1 ? name : basename(p),
              paths: [p],
            }))
          : [{ name, paths: importPaths }];
      for (const entry of entries) {
        const key = station?.key || `custom:${crypto.randomUUID()}`;
        custom.push({
          key,
          game: station?.game || '',
          station: entry.name,
          mode,
          // Store seconds internally so existing stations.json values remain compatible.
          crossfade: override === '' ? null : Number(override),
          spinLogo: $('#spinLogo').checked,
          bannerPath,
          bannerColor: $('#useBannerColor').checked ? $('#bannerColor').value : '',
          bannerAccent: bannerPath ? bannerAccent : '',
          custom: true,
          tracks: entry.paths.map((p) => ({ path: p, name: basename(p) })),
        });
        if (logoPath) logos[key] = logoPath;
        else delete logos[key];
      }
      stations = [
        ...stations.filter((s) => !s.custom && s.key !== station?.key && !hidden.includes(s.key)),
        ...custom,
      ];
      if (station) invalidateStationClock(station.key);
      save();
      render();
      if (station && selected?.key === station.key)
        tune(stations.find((s) => s.key === station.key));
      return true;
    },
  );
  $('#newMode').value = station?.mode || 'tracks';
  const updateCrossfadeVisibility = () => {
    $('#crossfadeField').hidden = $('#newMode').value !== 'tracks';
    const disabled = $('#useGlobalFade').checked || $('#crossfadeField').hidden;
    $('#stationFade').disabled = disabled;
    $('#stationFadeLabel').classList.toggle('disabled-field', disabled);
  };
  $('#newMode').onchange = updateCrossfadeVisibility;
  $('#useGlobalFade').onchange = updateCrossfadeVisibility;
  const updateBannerColor = () => {
    $('#bannerColor').disabled = !$('#useBannerColor').checked;
  };
  $('#useBannerColor').onchange = updateBannerColor;
  updateBannerColor();
  updateCrossfadeVisibility();
  listFiles();
  $('#browseFiles').onclick = async () => {
    const files = await window.radio.chooseFiles();
    importPaths = [...new Set([...importPaths, ...files])];
    listFiles();
  };
  $('#browseFolder').onclick = async () => {
    const folder = await window.radio.chooseAudioFolder();
    if (!folder) return;
    if (!$('#newName').value.trim()) $('#newName').value = folder.name;
    importPaths = [...new Set([...importPaths, ...folder.files])];
    listFiles();
  };
  $('#browseLogo').onclick = async () => {
    const p = await window.radio.chooseImage();
    if (p) {
      logoPath = p;
      $('#logoName').textContent = p;
    }
  };
  $('#removeLogo').onclick = () => {
    logoPath = '';
    $('#logoName').textContent = 'None';
  };
  $('#browseBanner').onclick = async () => {
    try {
      const result = await window.radio.chooseBanner();
      if (result) {
        bannerPath = result.path;
        bannerAccent = result.accent;
        $('#bannerName').textContent = bannerPath;
      }
    } catch (error) {
      $('#editorError').textContent = `Could not open banner: ${error.message}`;
    }
  };
  $('#removeBanner').onclick = () => {
    bannerPath = '';
    bannerAccent = '';
    $('#bannerName').textContent = 'No banner image';
  };
  if (station)
    $('#deleteStation').onclick = () => {
      custom = custom.filter((s) => s.key !== station.key);
      if (!station.custom && !hidden.includes(station.key)) hidden.push(station.key);
      stations = stations.filter((s) => s.key !== station.key);
      groups.forEach(
        (g) => (g.members = g.members.filter((m) => !memberBelongsToStation(m, station))),
      );
      delete logos[station.key];
      invalidateStationClock(station.key);
      if (selected?.key === station.key) {
        selected = null;
        ++token;
        audio.pause();
        audio.removeAttribute('src');
        audio.load();
        $('#stationName').textContent = 'Select a station';
      }
      save();
      render();
      $('#editor').close('cancel');
    };
}

function groupStationMarkup(station, members) {
  const stationChecked = members.has(station.key) ? 'checked' : '';
  const tracks =
    station.mode === 'tracks'
      ? station.tracks
          .map((track) => {
            const memberKey = station.key + '|' + track.path;
            const checked = members.has(memberKey) ? 'checked' : '';
            return `
            <label>
              <input class="member" type="checkbox" value="${esc(memberKey)}" ${checked}>
              ${esc(track.name)}
            </label>
          `;
          })
          .join('')
      : '';

  return `
    <details>
      <summary>
        <label>
          <input class="member" type="checkbox" value="${esc(station.key)}" ${stationChecked}>
          ${station.game ? `${esc(station.game)} / ` : ''}${esc(station.station)}
        </label>
      </summary>
      ${tracks}
    </details>
  `;
}

function groupEditor(g) {
  const existing = !!g,
    members = new Set(g?.members || []);
  editor(
    existing ? 'Edit group' : 'Create group',
    `
      <label>Group name <input id="groupName" value="${esc(g?.name || '')}" required></label>
      <p>Choose stations or individual tracks:</p>
      <div class="member-list">${stations.map((station) => groupStationMarkup(station, members)).join('')}</div>
      ${existing ? '<button type="button" id="deleteGroup">Delete group</button>' : ''}
    `,
    () => {
      const name = $('#groupName').value.trim();
      if (!name) return false;
      const chosen = [...document.querySelectorAll('.member:checked')].map((e) => e.value);
      if (existing) Object.assign(g, { name, members: chosen });
      else {
        g = { id: crypto.randomUUID(), name, members: chosen };
        groups.push(g);
      }
      for (const key of clocks.keys()) if (key.startsWith(g.id + ':')) clocks.delete(key);
      groupId = g.id;
      save();
      render();
      return true;
    },
  );
  if (existing)
    $('#deleteGroup').onclick = () => {
      groups = groups.filter((x) => x.id !== g.id);
      for (const key of clocks.keys()) if (key.startsWith(g.id + ':')) clocks.delete(key);
      groupId = 'all';
      save();
      $('#editor').close('cancel');
      render();
    };
}

function manageStations() {
  // Bulk deletion affects station entries and group references, never source audio files.
  const entries = [...stations];
  const rows = entries
    .map(
      (station) => `
        <div class="bulk-row">
          <label>
            <input type="checkbox" class="bulk-station" value="${esc(station.key)}">
            ${esc(station.station)}
            <small>(${station.tracks.length} file${station.tracks.length === 1 ? '' : 's'})</small>
          </label>
          <button type="button" class="station-edit" data-edit="${esc(station.key)}">Edit</button>
        </div>
      `,
    )
    .join('');

  editor(
    'Manage stations',
    `
      <p>Edit a station or select several to remove their entries. Audio files stay where they are.</p>
      <label><input type="checkbox" id="selectAllStations"> Select all ${entries.length} stations</label>
      <div class="member-list bulk-list">${rows || '<p>No stations yet.</p>'}</div>
      <p id="bulkCount">0 selected</p>
    `,
    () => {
      const keys = new Set(
        [...document.querySelectorAll('.bulk-station:checked')].map((e) => e.value),
      );
      if (!keys.size) return false;
      custom = custom.filter((s) => !keys.has(s.key));
      stations = stations.filter((s) => !keys.has(s.key));
      for (const key of keys)
        if (
          entries.some((station) => station.key === key && !station.custom) &&
          !hidden.includes(key)
        )
          hidden.push(key);
      groups.forEach(
        (g) =>
          (g.members = g.members.filter(
            (m) =>
              !entries.some(
                (station) => keys.has(station.key) && memberBelongsToStation(m, station),
              ),
          )),
      );
      for (const key of keys) delete logos[key];
      for (const key of keys) invalidateStationClock(key);
      if (selected && keys.has(selected.key)) {
        selected = null;
        ++token;
        audio.pause();
        audio.removeAttribute('src');
        audio.load();
        $('#stationName').textContent = 'Select a station';
        $('#trackTitle').textContent = 'Ready when you are';
        $('#trackAlbum').textContent = '';
        $('#logo').hidden = true;
      }
      save();
      render();
      return true;
    },
  );
  $('#editorSave').textContent = 'Delete selected';
  const deleteSelected = $('#editorSave').onclick;
  $('#editorSave').onclick = () => {
    const count = document.querySelectorAll('.bulk-station:checked').length;
    if (
      count &&
      window.confirm(
        `Remove ${count} station${count === 1 ? '' : 's'} from Radio Replay? Audio files will stay on disk.`,
      )
    )
      deleteSelected();
  };
  const update = () => {
    $('#bulkCount').textContent =
      `${document.querySelectorAll('.bulk-station:checked').length} selected`;
  };
  $('#selectAllStations').onchange = (e) => {
    document.querySelectorAll('.bulk-station').forEach((box) => (box.checked = e.target.checked));
    update();
  };
  document.querySelectorAll('.bulk-station').forEach((box) => (box.onchange = update));
  document.querySelectorAll('.station-edit').forEach(
    (button) =>
      (button.onclick = () => {
        const station = stations.find((s) => s.key === button.dataset.edit);
        $('#editor').close('cancel');
        stationEditor(station);
      }),
  );
}

// Wire controls after declaring the view and playback helpers above.
$('#import').onclick = () => stationEditor();
$('#newGroup').onclick = () => groupEditor();
$('#editGroup').onclick = () => groupEditor(groups.find((g) => g.id === groupId));
$('#homeButton').onclick = () => {
  $('#playerView').hidden = true;
  $('#homeView').hidden = false;
};
$('#manageStations').onclick = manageStations;
$('#openSettings').onclick = () => {
  $('#homeView').hidden = true;
  $('#playerView').hidden = true;
  $('#settingsView').hidden = false;
  applySettings();
};
$('#settingsBack').onclick = () => {
  $('#settingsView').hidden = true;
  if (selected) $('#playerView').hidden = false;
  else $('#homeView').hidden = false;
};
$('#accentColor').oninput = (e) => {
  settings.accent = e.target.value;
  applySettings();
};
$('#accentColor').onchange = () => save();
$('#matchBannerAccent').onchange = (event) => {
  settings.matchBannerAccent = event.target.checked;
  applyAccent(selected);
  save();
};
$('#globalFade').onchange = (e) => {
  settings.crossfade = Math.max(0, Math.min(30, Number(e.target.value) || 0));
  for (const [key, clock] of clocks) if (clock.usesGlobalFade) clocks.delete(key);
  applySettings();
  save();
};
$('#bounceSpeed').onchange = (e) => {
  settings.bounceSpeed = Math.max(10, Math.min(200, Number(e.target.value) || 40));
  applySettings();
  save();
};
$('#chooseStatic').onclick = async () => {
  const files = await window.radio.chooseFiles();
  if (files[0]) {
    settings.staticPath = files[0];
    applySettings();
    save();
  }
};
$('#useBundledStatic').onclick = () => {
  settings.staticPath = 'bundled';
  applySettings();
  save();
};
$('#clearStatic').onclick = () => {
  settings.staticPath = 'off';
  stopStatic();
  applySettings();
  save();
};
$('#groupSelect').onchange = (e) => {
  groupId = e.target.value;
  render();
  if (selected && visible().some((s) => s.key === selected.key)) tune(selected);
  else if (visible().length) tune(visible()[0]);
  else {
    ++token;
    audio.pause();
    audio.removeAttribute('src');
    audio.load();
  }
};
$('#editStation').onclick = () => {
  if (selected) stationEditor(selected);
};
$('#showConfig').onclick = () => window.radio.showConfig();
$('#play').onclick = () => {
  if (!audio.src && stations.length) tune(stations[0]);
  else if (audio.paused) {
    audio.play();
    if (fading) spare.play();
  } else {
    audio.pause();
    if (fading) {
      stopFade();
    }
  }
};
$('#prev').onclick = () => step(-1);
$('#next').onclick = () => step(1);
$('#volume').oninput = (e) => {
  audio.volume = e.target.value / 100;
  staticAudio.volume = e.target.value / 200;
};
audio.volume = 0.8;
for (const element of [audio, spare]) {
  element.addEventListener('timeupdate', () => {
    if (element !== audio) return;
    $('#time').textContent = `${fmt(audio.currentTime)} / ${fmt(audio.duration)}`;
    metadata();
    if (
      currentClock?.fade &&
      !fading &&
      Number.isFinite(audio.duration) &&
      audio.currentTime >= audio.duration - Math.max(10, currentClock.fade + 5)
    ) {
      const nextTrack = currentClock.order[(currentIndex + 1) % currentClock.order.length];
      if (nextTrack && !unavailable.has(nextTrack.path)) {
        const source = url(nextTrack.path);
        if (spare.src !== source) {
          spare.preload = 'auto';
          spare.src = source;
        }
      }
    }
    if (
      currentClock?.fade &&
      !fading &&
      Number.isFinite(audio.duration) &&
      audio.currentTime >= audio.duration - currentClock.fade - 0.15
    )
      startFade();
  });
  element.addEventListener('play', () => {
    if (element === audio) {
      $('#play').textContent = '❚❚';
      updateLogoMotion();
    }
  });
  element.addEventListener('pause', () => {
    if (element === audio) {
      $('#play').textContent = '▶';
      updateLogoMotion();
    }
  });
  element.addEventListener('ended', () => {
    if (element === audio && selected && !fading) advance();
  });
  element.addEventListener('error', () => {
    if (element !== audio || !selected || fading || !audio.src) return;
    const path = currentClock?.order[currentIndex]?.path;
    if (path && !unavailable.has(path)) {
      unavailable.add(path);
      updateUnavailableNotice();
      advance();
    }
  });
}
document.addEventListener('keydown', (e) => {
  if ($('#editor').open || !$('#settingsView').hidden || e.target.matches('input,select,textarea'))
    return;
  if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
    e.preventDefault();
    step(e.key === 'ArrowUp' ? -1 : 1);
  }
  if (e.code === 'Space') {
    e.preventDefault();
    $('#play').click();
  }
});
$('#playerView').addEventListener(
  'wheel',
  (e) => {
    const now = performance.now();
    if (Math.abs(e.deltaY) > 30 && now - lastWheelStep > 180) {
      lastWheelStep = now;
      step(e.deltaY > 0 ? 1 : -1);
    }
  },
  { passive: true },
);
document.addEventListener('dragover', (e) => e.preventDefault());
document.addEventListener('drop', async (e) => {
  e.preventDefault();
  if ($('#editor').open) return;
  try {
    const paths = (await window.radio.pathsForFiles(e.dataTransfer.files)).filter((p) =>
      /\.(mp3|m4a|aac|ogg|wav|flac)$/i.test(p),
    );
    if (paths.length) stationEditor(null, paths);
  } catch (error) {
    $('#libraryPath').textContent = `Could not import files: ${error.message}`;
  }
});

async function start() {
  // Migrate earlier settings without changing user-facing station names or keys.
  let configError = '';
  try {
    bundledStaticPath = await window.radio.bundledStatic();
    const config = await window.radio.loadConfig();
    if (config) {
      sanitizeConfig(config);
      let migrated = false;
      if (settings.staticPath === '') {
        settings.staticPath = 'bundled';
        migrated = true;
      }
      if (custom.some((s) => s.game === 'Custom')) {
        custom = custom.map((s) => (s.game === 'Custom' ? { ...s, game: '' } : s));
        migrated = true;
      }
      if (migrated) await save();
    } else await save();
    applySettings();
  } catch (error) {
    configError = `Settings error: ${error.message}. stations.json has not been changed.`;
  }
  if (root) await scan(root);
  else {
    stations = [...custom];
    $('#libraryPath').textContent = `${stations.length} stations`;
    render();
  }
  if (configError) $('#libraryPath').textContent = configError;
}
start();
