<script lang="ts">
  import { artworkUrl, type Station } from './library';
  let {station, logo, number = 1, playing = false, preview = false}: {station: Station; logo: unknown; number?: number; playing?: boolean; preview?: boolean} = $props();
  const color = $derived(typeof station.bannerColor === 'string' && /^#[0-9a-f]{6}$/i.test(station.bannerColor) ? station.bannerColor : '#122620');
</script>

<div class="station-banner" class:banner-preview={preview} style:background-color={color}>
  {#if artworkUrl(station.bannerPath)}<img class="banner-image" src={artworkUrl(station.bannerPath)} alt="" />{/if}
  <span class="frequency">FM / {String(number).padStart(2, '0')}</span>
  <div class="banner-content">
    <div class="station-logo" class:spin={station.spinLogo === true} class:spinning={playing}>
      {#if artworkUrl(logo)}<img src={artworkUrl(logo)} alt="" />{:else}<span>◉</span>{/if}
    </div>
    <div class="banner-title"><h1><strong>{station.station || 'YOUR STATION'}</strong></h1><p>CONTINUOUS BROADCAST</p><small>{station.tracks.length} TRACKS · {station.mode === 'tracks' ? 'SHUFFLE PROGRAM' : 'ORDERED PROGRAM'}</small></div>
  </div>
</div>
