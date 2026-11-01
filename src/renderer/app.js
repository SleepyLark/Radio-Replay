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
  tuningTransition: false,
  tuningMinMs: 1000,
  tuningMaxMs: 1000,
  crossfade: 0,
  randomNext: false,
  bounceSpeed: 40,
  matchBannerAccent: false,
  listDensity: 'comfortable',
  bannerLayout: 'column',
  showScanlines: true,
  reduceMotion: false,
  showProgress: true,
};
let bundledStaticPath = '';

let stations = [];
let selected = null;
let groupId = 'all';
let sidecar = null;
let displayedTrackPath = '';
let displayedTrackTitle = '';
let token = 0;
let importPaths = [];
let currentIndex = 0;
let currentClock = null;
let fading = false;
let fadeTimer = null;
let stepTimer = null;
let stepTarget = null;
let stepHistoryIndex = null;
let lastTunedStationKey = null;
let pendingStationVisit = null;
let stationHistory = { group: 'all', keys: [], cursor: -1, version: 0 };
let lastWheelStep = 0;

const clocks = new Map();
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
        textMode: ['titles', 'lyrics', 'ignore'].includes(station.textMode)
          ? station.textMode
          : 'auto',
        accent: /^#[0-9a-f]{6}$/i.test(station.accent) ? station.accent : '',
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
  const tuningMs = (value) =>
    Math.max(0, Math.min(1500, Math.round(Number.isFinite(value) ? value : 1000)));
  const minimum = tuningMs(loaded.tuningMinMs),
    maximum = tuningMs(loaded.tuningMaxMs);
  settings = {
    accent: typeof loaded.accent === 'string' ? loaded.accent : settings.accent,
    staticPath: typeof loaded.staticPath === 'string' ? loaded.staticPath : settings.staticPath,
    tuningTransition: loaded.tuningTransition === true,
    tuningMinMs: Math.min(minimum, maximum),
    tuningMaxMs: Math.max(minimum, maximum),
    crossfade: Number.isFinite(loaded.crossfade) ? loaded.crossfade : settings.crossfade,
    randomNext: loaded.randomNext === true,
    bounceSpeed: Number.isFinite(loaded.bounceSpeed) ? loaded.bounceSpeed : settings.bounceSpeed,
    matchBannerAccent: loaded.matchBannerAccent === true,
    listDensity: loaded.listDensity === 'compact' ? 'compact' : 'comfortable',
    // Preserve a previously chosen row layout when upgrading from station-level settings.
    bannerLayout:
      loaded.bannerLayout === 'row' ||
      (loaded.bannerLayout == null &&
        Array.isArray(config.custom) &&
        config.custom.some((station) => station.bannerLayout === 'row'))
        ? 'row'
        : 'column',
    showScanlines: loaded.showScanlines !== false,
    reduceMotion: loaded.reduceMotion === true,
    showProgress: loaded.showProgress !== false,
  };
  for (const station of custom) delete station.bannerLayout;
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
  $('#globalFade').value = Math.round((settings.crossfade ?? 0) * 1000);
  $('#bounceSpeed').value = settings.bounceSpeed ?? 40;
  $('#listDensity').value = settings.listDensity;
  $('#tuningTransition').checked = settings.tuningTransition;
  for (const key of ['tuningMinMs', 'tuningMaxMs']) {
    $('#' + key).value = settings[key];
    $('#' + key).disabled = !settings.tuningTransition;
  }
  $('#bannerLayout').value = settings.bannerLayout;
  $('.dial').classList.toggle('banner-row', settings.bannerLayout === 'row');
  $('#stationPreview')?.classList.toggle('banner-row', settings.bannerLayout === 'row');
  for (const key of ['showScanlines', 'reduceMotion', 'showProgress', 'randomNext'])
    $('#' + key).checked = settings[key];
  document.documentElement.dataset.density = settings.listDensity;
  document.documentElement.classList.toggle('reduce-motion', settings.reduceMotion);
  document.documentElement.classList.toggle('no-scanlines', !settings.showScanlines);
  $('#recordingProgress').hidden = !settings.showProgress;
  updateTitleBounce();
  $('#staticName').textContent =
    settings.staticPath === 'bundled'
      ? 'Bundled static.mp3'
      : settings.staticPath === 'off'
        ? 'Disabled'
        : settings.staticPath.split(/[\\/]/).at(-1);
}

function accentFor(station) {
  if (/^#[0-9a-f]{6}$/i.test(station?.accent)) return station.accent;
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
  dial.classList.toggle('banner-row', settings.bannerLayout === 'row');
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

let staticAbort = null;
function stopStatic() {
  staticAbort?.abort();
  staticAbort = null;
  staticAudio.pause();
  try {
    staticAudio.currentTime = 0;
  } catch {
    /* No loaded source to rewind. */
  }
}

function waitForStatic(event, ready, signal) {
  if (signal.aborted) return Promise.resolve(false);
  if (ready()) return Promise.resolve(true);
  return new Promise((resolve) => {
    const finish = (success) => {
      clearTimeout(timer);
      staticAudio.removeEventListener(event, loaded);
      staticAudio.removeEventListener('error', failed);
      signal.removeEventListener('abort', failed);
      resolve(success);
    };
    const loaded = () => finish(true),
      failed = () => finish(false);
    const timer = setTimeout(failed, 1500);
    staticAudio.addEventListener(event, loaded, { once: true });
    staticAudio.addEventListener('error', failed, { once: true });
    signal.addEventListener('abort', failed, { once: true });
  });
}

async function startStatic() {
  stopStatic();
  const transition = settings.tuningTransition;
  const delayMs = transition
    ? settings.tuningMinMs +
      Math.floor(Math.random() * (settings.tuningMaxMs - settings.tuningMinMs + 1))
    : 0;
  const path =
    settings.staticPath === 'bundled'
      ? bundledStaticPath
      : settings.staticPath === 'off'
        ? ''
        : settings.staticPath;
  if (path) {
    const controller = new AbortController();
    staticAbort = controller;
    const { signal } = controller;
    try {
      const source = url(path);
      // A cold preload="none" element won't fetch metadata until play()/load().
      // Random seeking needs that metadata before play, so explicitly start loading first.
      staticAudio.preload = 'metadata';
      if (staticAudio.src !== source || staticAudio.error) {
        staticAudio.src = source;
        staticAudio.load();
      } else if (staticAudio.readyState === 0 && staticAudio.networkState !== 2) {
        staticAudio.load();
      }
      staticAudio.loop = true;
      if (transition) {
        if (!(await waitForStatic('loadedmetadata', () => staticAudio.readyState >= 1, signal)))
          return;
        const duration = staticAudio.duration;
        staticAudio.currentTime = Number.isFinite(duration)
          ? Math.random() * Math.max(0, duration - delayMs / 1000)
          : 0;
        if (!(await waitForStatic('seeked', () => !staticAudio.seeking, signal))) return;
      } else staticAudio.currentTime = 0;
      if (signal.aborted) return;
      staticAudio.volume = Math.min(0.5, Number($('#volume').value) / 200);
      await staticAudio.play();
      if (signal.aborted) return;
      if (transition) {
        await new Promise((resolve) => {
          const finish = () => {
            clearTimeout(timer);
            signal.removeEventListener('abort', finish);
            resolve();
          };
          const timer = setTimeout(finish, delayMs);
          signal.addEventListener('abort', finish, { once: true });
        });
        // Keep looping until tune() finishes preparing the station, even after this minimum.
      }
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
  const description =
    station.mode === 'longset'
      ? 'broadcast'
      : `${station.tracks.length} track${station.tracks.length === 1 ? '' : 's'}`;

  return `
    <button class="station ${active}" data-key="${esc(station.key)}">
      <span>${esc(station.station)}</span>
      ${description ? `<small>${description}</small>` : ''}
    </button>
  `;
}

function homeStationMarkup(station) {
  const artwork = logos[station.key]
    ? `<img src="${esc(url(logos[station.key]))}" alt="" loading="lazy" decoding="async">`
    : '<span class="home-icon">◉</span>';
  const description =
    station.mode === 'longset'
      ? 'Continuous broadcast'
      : `${station.tracks.length} track${station.tracks.length === 1 ? '' : 's'}`;

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
  const query = $('#stationSearch').value.trim().toLocaleLowerCase();
  const shown = visible().filter((s) =>
    `${s.station} ${s.game || ''}`.toLocaleLowerCase().includes(query),
  );
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
        timed: Array.isArray(data.timed)
          ? data.timed
              .filter((x) => x && Number.isFinite(x.at) && x.at >= 0 && typeof x.text === 'string')
              .sort((a, b) => a.at - b.at)
          : [],
      };
    } catch {
      return { timed: [] };
    }
  }
  const timed = [];
  for (const line of raw.text.replace(/^\uFEFF/, '').split(/\r?\n/)) {
    const prefix = line.trimStart().match(/^(?:\[\d+:[0-5]\d(?:\.\d+)?\])+/)?.[0];
    if (!prefix) continue;
    const lyric = line.trimStart().slice(prefix.length).trim();
    for (const m of prefix.matchAll(/\[(\d+):([0-5]\d(?:\.\d+)?)\]/g)) {
      const at = +m[1] * 60 + +m[2];
      if (Number.isFinite(at)) timed.push({ at, text: lyric });
    }
  }
  return { timed: timed.sort((a, b) => a.at - b.at) };
}

const textModeFor = (station) =>
  ['titles', 'lyrics', 'ignore'].includes(station?.textMode)
    ? station.textMode
    : station?.mode === 'longset'
      ? 'titles'
      : 'lyrics';

function metadata() {
  const mode = textModeFor(selected);
  if (!sidecar?.timed?.length || mode === 'ignore') {
    $('#metadata').textContent = '';
    return;
  }

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
  if (mode === 'titles') {
    // The meaning belongs to the station, independent of how many recordings it holds.
    const title = currentLine || displayedTrackTitle;
    if ($('#trackTitle').textContent !== title) $('#trackTitle').textContent = title;
    $('#trackAlbum').textContent = '';
    $('#metadata').textContent = '';
    return;
  }

  const next = currentLine || '';
  if ($('#metadata').textContent !== next) $('#metadata').textContent = next;
}

async function showTrackDetails(track, station, currentToken) {
  // Both tuning and crossfade completion use this path so LRC timing follows the active MP3.
  displayedTrackPath = track.path;
  sidecar = null;
  displayedTrackTitle = track.name;
  $('#trackTitle').textContent = track.name;
  $('#trackAlbum').textContent = '';
  $('#metadata').textContent = '';

  const [sidecarFile, tags] = await Promise.all([
    textModeFor(station) === 'ignore' ? null : window.radio.readSidecar(track.path),
    window.radio.readTags(track.path),
  ]);
  if (currentToken !== token || displayedTrackPath !== track.path) return;

  sidecar = parse(sidecarFile);
  const timedInfo = sidecar?.timed?.length ? sidecar : null;
  const title = tags?.title || timedInfo?.title || track.name;
  const artist = tags?.artist || timedInfo?.artist;
  displayedTrackTitle = artist ? `${artist} - ${title}` : title;
  $('#trackTitle').textContent = displayedTrackTitle;
  $('#trackAlbum').textContent = tags?.album || timedInfo?.album || '';
  metadata();
}

async function probe(path) {
  // Metadata-only audio elements provide durations without buffering every track in RAM.
  let cached;
  try {
    cached = await window.radio.lookupDuration(path);
  } catch {
    /* Probe normally if the optional cache fails. */
  }
  if (cached?.available === false) {
    unavailable.add(path);
    return 0;
  }
  if (cached?.duration > 0) {
    unavailable.delete(path);
    return cached.duration;
  }
  const duration = await new Promise((resolve) => {
    const a = new Audio();
    let done = false;
    const timeout = setTimeout(() => finish(0), 10000);
    const finish = (n) => {
      if (done) return;
      done = true;
      clearTimeout(timeout);
      a.removeAttribute('src');
      a.load();
      resolve(n);
    };
    a.onloadedmetadata = () => finish(Number.isFinite(a.duration) ? a.duration : 0);
    a.onerror = () => finish(0);
    a.preload = 'metadata';
    a.src = url(path);
  });
  if (duration > 0 && cached)
    await window.radio.rememberDuration(path, cached, duration).catch(() => {});
  if (duration > 0) unavailable.delete(path);
  return duration;
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
  updateUnavailableNotice();
  return lengths;
}

async function clockFor(s, current) {
  // Revalidate durations on tune; retain the existing timeline when nothing changed.
  // `steps` are virtual play durations: each ends when the next track's overlap starts.
  const clockKey = `${groupId}:${s.key}`;
  const existing = clocks.get(clockKey);
  const included = new Set(groups.find((g) => g.id === groupId)?.members || []);
  const selectedTracks =
    groupId !== 'all' && !included.has(s.key)
      ? s.tracks.filter((t) => included.has(s.key + '|' + t.path))
      : s.tracks;
  const order =
    existing?.order || (s.mode === 'longset' ? [...selectedTracks] : shuffle(selectedTracks));
  const lengths = await probeLengths(order, current);
  if (existing && lengths.every((length, index) => length === existing.lengths[index])) {
    clocks.delete(clockKey);
    clocks.set(clockKey, existing);
    return existing;
  }
  const fade =
    order.length > 1 && lengths.every((n) => n > 0)
      ? Math.min(
          fadeFor(s),
          lengths.reduce((shortest, n) => Math.min(shortest, n / 2), Infinity),
        )
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
  clocks.delete(clockKey);
  // Bound retained session timelines; the currently playing station always remains available.
  if (order.length <= 20000) clocks.set(clockKey, clock);
  let retained = [...clocks.values()].reduce((sum, entry) => sum + entry.order.length, 0);
  for (const [key, entry] of clocks) {
    if (clocks.size <= 32 && retained <= 20000) break;
    if (entry === clock) continue;
    clocks.delete(key);
    retained -= entry.order.length;
  }
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
      commitStationVisit(station);
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

function resetStationHistory(seed = false) {
  const key =
    seed && visible().some((station) => station.key === lastTunedStationKey)
      ? lastTunedStationKey
      : null;
  stationHistory = {
    group: groupId,
    keys: key ? [key] : [],
    cursor: key ? 0 : -1,
    version: stationHistory.version + 1,
  };
  stepHistoryIndex = null;
}

function commitStationVisit(station) {
  const visit = pendingStationVisit;
  if (!visit || visit.key !== station.key || visit.group !== groupId) return;
  pendingStationVisit = null;
  lastTunedStationKey = station.key;
  if (!settings.randomNext) return;
  if (stationHistory.group !== groupId) resetStationHistory();
  let historyIndex = -1;
  if (visit.navigation?.version === stationHistory.version) historyIndex = visit.navigation.index;
  else if (visit.navigation) {
    // A pending tune can finish during the button debounce and shift the bounded history.
    const direction = visit.navigation.direction;
    for (
      let i = stationHistory.cursor + direction;
      i >= 0 && i < stationHistory.keys.length;
      i += direction
    ) {
      if (stationHistory.keys[i] === station.key) {
        historyIndex = i;
        break;
      }
    }
  }
  if (historyIndex >= 0 && stationHistory.keys[historyIndex] === station.key) {
    stationHistory.cursor = historyIndex;
  } else if (stationHistory.keys[stationHistory.cursor] !== station.key) {
    stationHistory.keys = stationHistory.keys.slice(0, stationHistory.cursor + 1);
    stationHistory.keys.push(station.key);
    stationHistory.cursor = stationHistory.keys.length - 1;
  }
  const start = Math.max(0, stationHistory.cursor - 2);
  stationHistory.keys = stationHistory.keys.slice(start, stationHistory.cursor + 3);
  stationHistory.cursor -= start;
  stationHistory.version++;
}

async function tune(s, navigation = null) {
  // The token invalidates pending metadata/sidecar work when tuning changes quickly.
  if (!s) return;
  $('#settingsView').hidden = true;
  $('#homeView').hidden = true;
  $('#playerView').hidden = false;
  const current = ++token;
  clearTimeout(stepTimer);
  stepTimer = null;
  stepTarget = null;
  stepHistoryIndex = null;
  pendingStationVisit = { key: s.key, group: groupId, navigation };
  stopFade();
  audio.pause();
  currentClock = null;
  sidecar = null;
  displayedTrackPath = '';
  selected = s;
  const useTransition = settings.tuningTransition;
  const tuningSound = startStatic();
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
  $('#recordingProgress').value = 0;
  updateActiveStation();
  try {
    const clock = await clockFor(s, current);
    if (useTransition) await tuningSound;
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
    if (current === token) commitStationVisit(s);
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
  let historyIndex = null,
    target = null;
  if (settings.randomNext) {
    if (stationHistory.group !== groupId) resetStationHistory();
    let cursor = stepHistoryIndex ?? stationHistory.cursor;
    // Undo a queued/new tune back to the last successful station before going further back.
    if (
      n < 0 &&
      stepHistoryIndex == null &&
      (stepTarget || pendingStationVisit) &&
      (stepTarget?.key || pendingStationVisit?.key) !== stationHistory.keys[cursor]
    )
      cursor++;
    for (let i = cursor + n; i >= 0 && i < stationHistory.keys.length; i += n) {
      const candidate = list.find((station) => station.key === stationHistory.keys[i]);
      if (candidate && candidate.tracks.some((track) => !unavailable.has(track.path))) {
        target = candidate;
        historyIndex = i;
        break;
      }
    }
    if (!target && n < 0) return;
    if (!target) {
      const candidates = list.filter(
        (station) => station.key !== (stepTarget?.key || selected?.key),
      );
      target = candidates.length
        ? candidates[Math.floor(Math.random() * candidates.length)]
        : list[0];
    }
  } else target = list[(Math.max(0, index) + n + list.length) % list.length];
  stepTarget = target;
  stepHistoryIndex = historyIndex;
  const navigation =
    historyIndex == null
      ? null
      : { index: historyIndex, version: stationHistory.version, direction: Math.sign(n) };
  $('#frequency').textContent = String(list.indexOf(stepTarget) + 1).padStart(2, '0');
  $('#stationName').textContent = stepTarget.station;
  clearTimeout(stepTimer);
  stepTimer = setTimeout(() => tune(target, navigation), 180);
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
  dialog.ondrop = null;
  $('#editorSave').textContent = 'Save';
  $('#editorSave').disabled = false;
  $('#editorSave').classList.remove('danger');
  $('#editorBody').scrollTop = 0;
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
    <div id="stationPreview" class="station-preview" aria-label="Station appearance preview">
      <img id="previewBanner" alt="" hidden>
      <div class="preview-content"><img id="previewLogo" alt="" hidden><span id="previewPlaceholder">◉</span>
        <strong id="previewName"></strong><small id="previewMode"></small></div>
    </div>
    <details class="editor-section" open><summary>Essentials</summary>
    ${
      !station
        ? `<label>Create
      <select id="newGrouping"><option value="combined">One station with all files</option><option value="separate">One station per file</option></select>
    </label><p id="creationPreview"></p>`
        : ''
    }
    <div id="stationIdentity">
    <label>Station name
      <input id="newName" value="${esc(station?.station || '')}" placeholder="My radio station">
    </label>
    </div>
    <div class="preset-actions" aria-label="Station presets">
      <button type="button" data-preset="tracks">♫ Music station</button>
      <button type="button" data-preset="longset">◉ Radio broadcast</button>
    </div>
    <label>Playback style
      <select id="newMode">
        <option value="tracks">Shuffle tracks</option>
        <option value="longset">Broadcast — play in order and loop</option>
      </select>
    </label>
    <p id="playbackHelp"></p>
    <div id="broadcastSuggestion" class="suggestion" hidden>These look like broadcasts. Play them in order?
      <button type="button" id="acceptBroadcast">Use broadcast</button>
    </div>
    <div class="file-actions">
      <button type="button" id="browseFiles">Add files</button>
      <button type="button" id="browseFolder">Add folder</button>
    </div>
    <p id="fileCount"></p>
    <p id="fileOrderHelp">Numbers show the file order for broadcasts. Use ↑ / ↓ to reorder.</p>
    <div id="fileList" tabindex="0" role="region" aria-label="Audio files grouped by folder"></div>
    </details>
    <details id="stationAppearance" class="editor-section"><summary>Appearance</summary>
    <div class="editor-section">
      <strong>Station logo</strong>
      <p>Set a PNG, JPG, WebP, or animated GIF for the station.</p> 
      <div id="logoName">${esc(logoPath || 'None')}</div>
      <button type="button" id="browseLogo">Choose logo</button>
      <button type="button" id="removeLogo" ${logoPath ? '' : 'disabled'}>Remove logo</button>
      <label class="editor-check"><input id="spinLogo" type="checkbox" ${station?.spinLogo ? 'checked' : ''}> Spin logo while playing</label>
    </div>
    <div class="editor-section">
      <strong>Station banner</strong>
      <p>Use an image or color across the radio display behind the station name.</p>
      <div id="bannerName">${esc(bannerPath || 'No banner image')}</div>
      <button type="button" id="browseBanner">Choose banner image</button>
      <button type="button" id="removeBanner" ${bannerPath ? '' : 'disabled'}>Remove image</button>
      <label class="editor-check"><input id="useBannerColor" type="checkbox" ${station?.bannerColor ? 'checked' : ''}> Use a banner color</label>
      <label class="inline-color">Banner color <input id="bannerColor" type="color" value="${esc(station?.bannerColor || '#244b42')}"></label>
    </div>
    <label class="editor-check"><input id="useStationAccent" type="checkbox" ${station?.accent ? 'checked' : ''}> Override player accent</label>
    <label class="inline-color">Station accent <input id="stationAccent" type="color" value="${esc(station?.accent || settings.accent)}"></label>
    <p>Unchecked: follows Settings → Appearance. <button type="button" id="resetAccent" class="ghost">Reset to default</button></p>
    </details>
    <details class="editor-section"><summary>Advanced</summary>
    <label>Use timestamped text as
      <select id="textMode"><option value="auto">Default from playback style</option><option value="titles">Track titles</option><option value="lyrics">Lyrics</option><option value="ignore">Ignore sidecars</option></select>
    </label>
    <p id="textModeHelp"></p>
    <p>Only text with valid timestamps is displayed. Untimed notes stay out of the player.</p>
    <div id="crossfadeField" hidden class="editor-section">
      <label class="editor-check"><input id="useGlobalFade" type="checkbox" ${station?.crossfade == null ? 'checked' : ''}> Default crossfade (from Settings: ${esc(Math.round(settings.crossfade * 1000))} ms)</label>
      <label id="stationFadeLabel" class="inline-number">Station crossfade <input id="stationFade" type="number" min="0" max="30000" step="1"
        value="${station?.crossfade == null ? '0' : Math.round(Math.min(30, station.crossfade) * 1000)}"> ms</label>
    </div>
    </details>
    ${station ? '<button type="button" id="deleteStation" class="danger">Delete station</button>' : ''}
    <p id="editorError" class="error"></p>
  `;
}

const isInFolder = (path, folder) => path.replace(/\\/g, '/').startsWith(folder + '/');

function trackTreeMarkup(paths, folderStates) {
  const parents = paths.map((p) => p.replace(/\\/g, '/').split('/').slice(0, -1));
  let shared = parents[0]?.length || 0;
  for (const parts of parents)
    while (shared && parts.slice(0, shared).join('/') !== parents[0].slice(0, shared).join('/'))
      shared--;
  const root = { folders: new Map(), files: [] };
  paths.forEach((path, index) => {
    const parts = parents[index];
    let node = root;
    // Show the common parent once, then its subfolders. Keep the stored playback order intact.
    for (let depth = Math.max(0, shared - 1); depth < parts.length; depth++) {
      if (!parts[depth]) continue;
      const key = parts.slice(0, depth + 1).join('/');
      if (!node.folders.has(key))
        node.folders.set(key, {
          name: parts[depth],
          folders: new Map(),
          files: [],
          count: 0,
        });
      node = node.folders.get(key);
      node.count++;
    }
    node.files.push({ path, index });
  });
  const markup = (node) =>
    [...node.folders]
      .map(
        ([key, folder], index, siblings) => `
    <details class="file-folder" data-folder="${esc(key)}" ${folderStates.get(key) !== false ? 'open' : ''}>
      <summary title="${esc(key)}"><span>${esc(folder.name)}</span><small>${folder.count} file${folder.count === 1 ? '' : 's'}</small>
        <button type="button" data-folder-up="${esc(key)}" data-folder-target="${esc(siblings[index - 1]?.[0] || '')}" aria-label="Move ${esc(folder.name)} folder up" ${index === 0 ? 'disabled' : ''}>↑</button>
        <button type="button" data-folder-down="${esc(key)}" data-folder-target="${esc(siblings[index + 1]?.[0] || '')}" aria-label="Move ${esc(folder.name)} folder down" ${index === siblings.length - 1 ? 'disabled' : ''}>↓</button>
        <button type="button" data-folder-first="${esc(key)}" aria-label="Move ${esc(folder.name)} folder to first" ${paths.slice(0, folder.count).every((path) => isInFolder(path, key)) ? 'disabled' : ''}>⇈ First</button>
      </summary>
      <div class="folder-files">${markup(folder)}</div>
    </details>`,
      )
      .join('') +
    node.files
      .map(({ path, index }) => {
        const name = path
          .split(/[\\/]/)
          .at(-1)
          .replace(/\.[^.]+$/, '');
        return `<div class="file-row" data-track-path="${esc(path)}">
        <small class="file-number">${String(index + 1).padStart(2, '0')}</small>
        <span title="${esc(path)}">${esc(name)}</span><div class="file-row-actions">
          <button type="button" data-up="${index}" aria-label="Move ${esc(name)} up" ${index === 0 ? 'disabled' : ''}>↑</button>
          <button type="button" data-down="${index}" aria-label="Move ${esc(name)} down" ${index === paths.length - 1 ? 'disabled' : ''}>↓</button>
          <button type="button" data-remove="${index}" aria-label="Remove ${esc(name)}">Remove</button>
        </div></div>`;
      })
      .join('');
  return markup(root);
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
  const separate = () => !station && $('#newGrouping').value === 'separate';
  const folderStates = new Map();
  const listFiles = () => {
    $('#fileCount').textContent =
      `${importPaths.length} audio file${importPaths.length === 1 ? '' : 's'}`;
    const list = $('#fileList');
    const scrollTop = list.scrollTop;
    for (const folder of list.querySelectorAll('[data-folder]'))
      folderStates.set(folder.dataset.folder, folder.open);
    list.innerHTML =
      trackTreeMarkup(importPaths, folderStates) ||
      '<div class="file-drop-zone">Drop audio files here, or use Add files / Add folder.</div>';
    list.scrollTop = scrollTop;
    for (const direction of ['up', 'down']) {
      list.querySelectorAll(`[data-folder-${direction}]`).forEach((button) => {
        button.onclick = (event) => {
          event.preventDefault();
          event.stopPropagation();
          const folder = button.dataset[direction === 'up' ? 'folderUp' : 'folderDown'];
          const target = button.dataset.folderTarget;
          if (!target) return;
          const moving = importPaths.filter((path) => isInFolder(path, folder));
          const remaining = importPaths.filter((path) => !isInFolder(path, folder));
          const positions = remaining
            .map((path, index) => (isInFolder(path, target) ? index : -1))
            .filter((index) => index >= 0);
          const insertAt = direction === 'up' ? positions[0] : positions.at(-1) + 1;
          remaining.splice(insertAt, 0, ...moving);
          importPaths = remaining;
          listFiles();
        };
      });
    }
    list.querySelectorAll('[data-folder-first]').forEach((button) => {
      button.onclick = (event) => {
        event.preventDefault();
        event.stopPropagation();
        const folder = button.dataset.folderFirst;
        // Stable partition includes subfolders and preserves each recording's relative order.
        importPaths = [
          ...importPaths.filter((path) => isInFolder(path, folder)),
          ...importPaths.filter((path) => !isInFolder(path, folder)),
        ];
        listFiles();
        list.scrollTop = 0;
      };
    });
    document.querySelectorAll('[data-remove]').forEach(
      (b) =>
        (b.onclick = () => {
          importPaths.splice(Number(b.dataset.remove), 1);
          listFiles();
        }),
    );
    for (const direction of ['up', 'down'])
      document.querySelectorAll(`[data-${direction}]`).forEach((button) => {
        button.onclick = () => {
          const index = Number(button.dataset[direction]);
          const next = index + (direction === 'up' ? -1 : 1);
          [importPaths[index], importPaths[next]] = [importPaths[next], importPaths[index]];
          listFiles();
        };
      });
    updateForm();
  };
  editor(
    station ? 'Edit station' : 'Create station',
    stationFormMarkup(station, logoPath, bannerPath),
    () => {
      const name = $('#newName').value.trim(),
        mode = $('#newMode').value;
      if ((!separate() && !name) || !importPaths.length) {
        $('#editorError').textContent = separate()
          ? 'Add at least one audio file.'
          : 'Enter a name and add at least one audio file.';
        return false;
      }
      const customFade = mode === 'tracks' && !$('#useGlobalFade').checked;
      const override = customFade ? $('#stationFade').value.trim() : '';
      if (
        customFade &&
        (!override ||
          !Number.isFinite(Number(override)) ||
          Number(override) < 0 ||
          Number(override) > 30000 ||
          !Number.isInteger(Number(override)))
      ) {
        $('#editorError').textContent = 'Crossfade must be between 0 and 30000 milliseconds.';
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
      const entries = separate()
        ? importPaths.map((p) => ({
            name: basename(p),
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
          textMode: $('#textMode').value,
          // Store seconds internally so existing stations.json values remain compatible.
          crossfade: override === '' ? null : Number(override) / 1000,
          spinLogo: !separate() && $('#spinLogo').checked,
          bannerPath: separate() ? '' : bannerPath,
          bannerColor: !separate() && $('#useBannerColor').checked ? $('#bannerColor').value : '',
          bannerAccent: !separate() && bannerPath ? bannerAccent : '',
          accent: !separate() && $('#useStationAccent').checked ? $('#stationAccent').value : '',
          custom: true,
          tracks: entry.paths.map((p) => ({ path: p, name: basename(p) })),
        });
        if (logoPath && !separate()) logos[key] = logoPath;
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
  $('#textMode').value = station?.textMode || 'auto';
  const updatePreview = () => {
    $('#stationPreview').classList.toggle('banner-row', settings.bannerLayout === 'row');
    $('#previewName').textContent = $('#newName').value.trim() || 'YOUR STATION';
    $('#previewMode').textContent =
      $('#newMode').value === 'longset' ? 'BROADCAST · IN ORDER · LOOP' : 'MUSIC · SHUFFLE';
    $('#stationPreview').style.backgroundColor = $('#useBannerColor').checked
      ? $('#bannerColor').value
      : '#0b1513';
    $('#stationPreview').style.color = $('#useStationAccent').checked
      ? $('#stationAccent').value
      : settings.accent;
    $('#previewBanner').hidden = !bannerPath;
    if (bannerPath) $('#previewBanner').src = url(bannerPath);
    $('#previewLogo').hidden = !logoPath;
    $('#previewPlaceholder').hidden = !!logoPath;
    if (logoPath) $('#previewLogo').src = url(logoPath);
    $('#removeLogo').disabled = !logoPath;
    $('#removeBanner').disabled = !bannerPath;
    $('#stationAccent').disabled = !$('#useStationAccent').checked;
  };
  const updateCrossfadeVisibility = () => {
    $('#crossfadeField').hidden = $('#newMode').value !== 'tracks';
    const disabled = $('#useGlobalFade').checked || $('#crossfadeField').hidden;
    $('#stationFade').disabled = disabled;
    $('#stationFadeLabel').classList.toggle('disabled-field', disabled);
  };
  const updateForm = () => {
    const batch = separate();
    for (const selector of ['#stationIdentity', '#stationAppearance', '#stationPreview'])
      $(selector).hidden = batch;
    if ($('#creationPreview'))
      $('#creationPreview').textContent = batch
        ? `${importPaths.length} station${importPaths.length === 1 ? '' : 's'} will be created, named after their files. Add logos and banners afterwards in Edit station.`
        : 'All files share one name, logo, and playback style.';
    $('#editorSave').textContent = station
      ? 'Save changes'
      : batch
        ? `Create ${importPaths.length} stations`
        : 'Create station';
    const broadcast = $('#newMode').value === 'longset';
    $('#fileOrderHelp').hidden = !broadcast || batch;
    $('#playbackHelp').textContent = broadcast
      ? 'Recordings play in the order below, then loop. Timestamps supply song titles by default.'
      : 'Files play in a shuffled order. Timestamps supply lyrics by default.';
    $('#textModeHelp').textContent =
      `Default from playback style: ${broadcast ? 'Track titles' : 'Lyrics'}. This applies to every file in the station.`;
    document
      .querySelectorAll('[data-preset]')
      .forEach((button) =>
        button.setAttribute('aria-pressed', String(button.dataset.preset === $('#newMode').value)),
      );
    if (broadcast) $('#broadcastSuggestion').hidden = true;
    updateCrossfadeVisibility();
    updatePreview();
  };
  $('#newMode').onchange = updateForm;
  if ($('#newGrouping')) $('#newGrouping').onchange = updateForm;
  document.querySelectorAll('[data-preset]').forEach((button) => {
    button.onclick = () => {
      $('#newMode').value = button.dataset.preset;
      $('#textMode').value = 'auto';
      $('#useGlobalFade').checked = true;
      updateForm();
    };
  });
  $('#acceptBroadcast').onclick = () => {
    $('#newMode').value = 'longset';
    updateForm();
  };
  $('#newName').oninput = updatePreview;
  $('#stationAccent').oninput = updatePreview;
  $('#useStationAccent').onchange = updatePreview;
  $('#resetAccent').onclick = () => {
    $('#useStationAccent').checked = false;
    updatePreview();
  };
  $('#useGlobalFade').onchange = updateCrossfadeVisibility;
  const updateBannerColor = () => {
    $('#bannerColor').disabled = !$('#useBannerColor').checked;
    updatePreview();
  };
  $('#bannerColor').oninput = updatePreview;
  $('#useBannerColor').onchange = updateBannerColor;
  updateBannerColor();
  updateCrossfadeVisibility();
  listFiles();
  // Check a small sample without delaying setup or scanning the entire library.
  const modeField = $('#newMode');
  const inspected = new Set();
  const suggestBroadcast = async () => {
    if (station) return;
    const sample = importPaths.filter((p) => !inspected.has(p)).slice(0, 3);
    sample.forEach((p) => inspected.add(p));
    const hints = await Promise.all(
      sample.map(async (p) => {
        try {
          const [raw, length] = await Promise.all([window.radio.readSidecar(p), probe(p)]);
          return parse(raw)?.timed?.length > 0 || length >= 20 * 60;
        } catch {
          return false;
        }
      }),
    );
    if (
      $('#newMode') === modeField &&
      $('#editor').open &&
      $('#newMode').value === 'tracks' &&
      hints.some(Boolean)
    )
      $('#broadcastSuggestion').hidden = false;
  };
  suggestBroadcast();
  $('#editor').ondrop = async (event) => {
    event.preventDefault();
    event.stopPropagation();
    try {
      // FileList loses its entries across the context bridge; File arrays retain disk paths.
      const paths = await window.radio.pathsForFiles(Array.from(event.dataTransfer.files));
      if ($('#newMode') !== modeField || !$('#editor').open) return;
      if (!paths.length) {
        $('#editorError').textContent =
          'Drop audio files (MP3, M4A, AAC, OGG, WAV, or FLAC). To import a folder, use Add folder.';
        return;
      }
      $('#editorError').textContent = '';
      importPaths = [...new Set([...importPaths, ...paths])];
      listFiles();
      suggestBroadcast();
    } catch (error) {
      if ($('#newMode') === modeField && $('#editor').open)
        $('#editorError').textContent = `Could not import files: ${error.message}`;
    }
  };
  $('#browseFiles').onclick = async () => {
    const files = await window.radio.chooseFiles();
    importPaths = [...new Set([...importPaths, ...files])];
    listFiles();
    suggestBroadcast();
  };
  $('#browseFolder').onclick = async () => {
    const folder = await window.radio.chooseAudioFolder();
    if (!folder) return;
    if (!$('#newName').value.trim()) $('#newName').value = folder.name;
    importPaths = [...new Set([...importPaths, ...folder.files])];
    listFiles();
    suggestBroadcast();
  };
  $('#browseLogo').onclick = async () => {
    const p = await window.radio.chooseImage();
    if (p) {
      logoPath = p;
      $('#logoName').textContent = p;
      updatePreview();
    }
  };
  $('#removeLogo').onclick = () => {
    logoPath = '';
    $('#logoName').textContent = 'None';
    updatePreview();
  };
  $('#browseBanner').onclick = async () => {
    try {
      const result = await window.radio.chooseBanner();
      if (result) {
        bannerPath = result.path;
        bannerAccent = result.accent;
        $('#bannerName').textContent = bannerPath;
        updatePreview();
      }
    } catch (error) {
      $('#editorError').textContent = `Could not open banner: ${error.message}`;
    }
  };
  $('#removeBanner').onclick = () => {
    bannerPath = '';
    bannerAccent = '';
    $('#bannerName').textContent = 'No banner image';
    updatePreview();
  };
  $('#newName').focus({ preventScroll: true });
  $('#editorBody').scrollTop = 0;
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
  $('#editorSave').classList.add('danger');
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
    const count = document.querySelectorAll('.bulk-station:checked').length;
    $('#bulkCount').textContent = `${count} selected`;
    $('#editorSave').disabled = count === 0;
    $('#editorSave').textContent = count
      ? `Delete ${count} station${count === 1 ? '' : 's'}`
      : 'Delete selected';
    $('#selectAllStations').checked = count > 0 && count === entries.length;
    $('#selectAllStations').indeterminate = count > 0 && count < entries.length;
  };
  update();
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
$('#stationSearch').oninput = render;
for (const button of document.querySelectorAll('[data-settings-tab]')) {
  button.onclick = () => {
    for (const tab of document.querySelectorAll('[data-settings-tab]'))
      tab.setAttribute('aria-pressed', String(tab === button));
    for (const panel of document.querySelectorAll('[data-settings-panel]'))
      panel.hidden = panel.dataset.settingsPanel !== button.dataset.settingsTab;
  };
}
for (const key of [
  'listDensity',
  'bannerLayout',
  'showScanlines',
  'reduceMotion',
  'showProgress',
  'tuningTransition',
  'randomNext',
]) {
  $('#' + key).onchange = (event) => {
    settings[key] = ['listDensity', 'bannerLayout'].includes(key)
      ? event.target.value
      : event.target.checked;
    if (key === 'tuningTransition') stopStatic();
    if (key === 'randomNext') resetStationHistory(settings.randomNext);
    applySettings();
    save();
  };
}
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
  settings.crossfade = Math.max(0, Math.min(30000, Math.round(Number(e.target.value) || 0))) / 1000;
  for (const [key, clock] of clocks) if (clock.usesGlobalFade) clocks.delete(key);
  applySettings();
  save();
};
for (const key of ['tuningMinMs', 'tuningMaxMs']) {
  $('#' + key).onchange = (event) => {
    settings[key] = Math.max(0, Math.min(1500, Math.round(Number(event.target.value) || 0)));
    if (key === 'tuningMinMs')
      settings.tuningMaxMs = Math.max(settings.tuningMinMs, settings.tuningMaxMs);
    else settings.tuningMinMs = Math.min(settings.tuningMinMs, settings.tuningMaxMs);
    applySettings();
    save();
  };
}
$('#clearDurationCache').onclick = async () => {
  const button = $('#clearDurationCache');
  button.disabled = true;
  $('#cacheStatus').textContent = 'Clearing duration cache…';
  try {
    await window.radio.clearDurationCache();
    clocks.clear();
    $('#cacheStatus').textContent = 'Duration cache cleared. Durations will be rebuilt as needed.';
  } catch (error) {
    $('#cacheStatus').textContent = `Could not clear duration cache: ${error.message}`;
  } finally {
    button.disabled = false;
  }
};
$('#bounceSpeed').onchange = (e) => {
  settings.bounceSpeed = Math.max(10, Math.min(200, Number(e.target.value) || 40));
  applySettings();
  save();
};
$('#chooseStatic').onclick = async () => {
  const files = await window.radio.chooseFiles();
  if (files[0]) {
    stopStatic();
    settings.staticPath = files[0];
    applySettings();
    save();
  }
};
$('#useBundledStatic').onclick = () => {
  stopStatic();
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
  clearTimeout(stepTimer);
  stepTimer = null;
  stepTarget = null;
  groupId = e.target.value;
  resetStationHistory();
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
let muted = false;
function updateVolume() {
  const value = Number($('#volume').value);
  if (!fading) audio.volume = value / 100;
  staticAudio.volume = value / 200;
  for (const element of [audio, spare, staticAudio]) element.muted = muted;
  $('#volumeValue').textContent = muted ? 'MUTE' : `${value}%`;
  $('#volume').setAttribute('aria-valuetext', `${value} percent${muted ? ', muted' : ''}`);
  $('#mute').setAttribute('aria-pressed', String(muted));
  $('#mute').setAttribute('aria-label', muted ? 'Unmute' : 'Mute');
  $('#mute').dataset.silent = String(muted || value === 0);
  const count = muted ? 0 : Math.round((value * 12) / 100);
  document
    .querySelectorAll('.volume-segments i')
    .forEach((segment, index) => segment.classList.toggle('lit', index < count));
}
function changeVolume(delta) {
  $('#volume').value = Math.max(0, Math.min(100, Number($('#volume').value) + delta));
  muted = false;
  updateVolume();
}
$('.volume-segments').innerHTML = Array.from(
  { length: 12 },
  (_, index) => `<i data-level="${index + 1}"></i>`,
).join('');
document.querySelectorAll('.volume-segments i').forEach((bar, index) => {
  bar.style.height = `${6 + (index * 18) / 11}px`;
});
$('#volume').oninput = () => {
  muted = false;
  updateVolume();
};
$('#mute').onclick = () => {
  if (Number($('#volume').value) === 0) {
    $('#volume').value = 80;
    muted = false;
  } else muted = !muted;
  updateVolume();
};
$('.volume-module').addEventListener(
  'wheel',
  (event) => {
    event.preventDefault();
    event.stopPropagation();
    if (event.deltaY) changeVolume(event.deltaY < 0 ? 5 : -5);
  },
  { passive: false },
);
updateVolume();
for (const element of [audio, spare]) {
  element.addEventListener('timeupdate', () => {
    if (element !== audio) return;
    $('#time').textContent = `${fmt(audio.currentTime)} / ${fmt(audio.duration)}`;
    $('#recordingProgress').value =
      Number.isFinite(audio.duration) && audio.duration > 0
        ? audio.currentTime / audio.duration
        : 0;
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
const stationMediaActions = {
  nexttrack: () => step(1),
  previoustrack: () => step(-1),
  play: () => {
    if (audio.paused) $('#play').click();
  },
  pause: () => {
    if (!audio.paused) $('#play').click();
  },
};
for (const [action, handler] of Object.entries(stationMediaActions))
  navigator.mediaSession?.setActionHandler(action, handler);

document.addEventListener('keydown', (e) => {
  if (e.key === 'MediaPlayPause') {
    e.preventDefault();
    $('#play').click();
    return;
  }
  const mediaAction = {
    MediaTrackNext: 'nexttrack',
    MediaTrackPrevious: 'previoustrack',
    MediaPlay: 'play',
    MediaPause: 'pause',
  }[e.key];
  if (mediaAction) {
    e.preventDefault();
    stationMediaActions[mediaAction]();
    return;
  }
  if ($('#editor').open || !$('#settingsView').hidden || e.target.matches('input,select,textarea'))
    return;
  if (e.ctrlKey || e.metaKey || e.altKey) return;
  if (['+', '=', '-', '_'].includes(e.key)) {
    e.preventDefault();
    changeVolume(e.key === '+' || e.key === '=' ? 5 : -5);
  }
  if (e.key.toLowerCase() === 'm') {
    e.preventDefault();
    $('#mute').click();
  }
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
document.addEventListener('dragover', (e) => {
  e.preventDefault();
  if (e.dataTransfer?.types.includes('Files')) {
    e.dataTransfer.dropEffect = 'copy';
    document.documentElement.classList.add('file-drag-active');
  }
});
const clearDropHighlight = () => document.documentElement.classList.remove('file-drag-active');
document.addEventListener('drop', clearDropHighlight, true);
document.addEventListener('dragend', clearDropHighlight);
document.addEventListener('dragleave', (event) => {
  if (!event.relatedTarget) clearDropHighlight();
});
document.addEventListener('drop', async (e) => {
  e.preventDefault();
  if ($('#editor').open || !e.dataTransfer.files.length) return;
  try {
    const paths = (await window.radio.pathsForFiles(Array.from(e.dataTransfer.files))).filter((p) =>
      /\.(mp3|m4a|aac|ogg|wav|flac)$/i.test(p),
    );
    if ($('#editor').open) return;
    if (paths.length) stationEditor(null, paths);
    else $('#libraryPath').textContent = 'Drop audio files, or use Create station → Add folder.';
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
