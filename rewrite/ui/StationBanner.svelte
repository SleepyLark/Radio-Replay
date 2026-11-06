<script lang="ts">
  import { artworkUrl, type Station } from './library';
  let {station, logo, number = 1, playing = false, preview = false, layout = 'row'}: {station: Station; logo: unknown; number?: number; playing?: boolean; preview?: boolean; layout?: unknown} = $props();
  const color = $derived(typeof station.bannerColor === 'string' && /^#[0-9a-f]{6}$/i.test(station.bannerColor) ? station.bannerColor : '#0b1513');
  const subtitle = $derived(station.mode === 'tracks' ? `${station.tracks.length} TRACKS • SHUFFLE PROGRAM` : 'CONTINUOUS BROADCAST');
</script>

{#if preview}
  <div class="station-preview banner-preview" class:banner-row={layout === 'row'} style:background-color={color}>
    {#if artworkUrl(station.bannerPath)}<img id="previewBanner" src={artworkUrl(station.bannerPath)} alt=""/>{/if}
    <div class="preview-content">
      {#if artworkUrl(logo)}<img id="previewLogo" src={artworkUrl(logo)} alt=""/>{:else}<span id="previewPlaceholder">◉</span>{/if}
      <div id="previewName">{station.station || 'Your station'}</div>
      <div id="previewMode">{subtitle}</div>
    </div>
  </div>
{:else}
  <div class="dial" class:banner-row={layout === 'row'} style:background-color={color}>
    <img id="dialBanner" src={artworkUrl(station.bannerPath)||undefined} alt="" hidden={!artworkUrl(station.bannerPath)}/>
    <div class="scanlines"></div>
    <img id="logo" src={artworkUrl(logo)||undefined} alt="" hidden={!artworkUrl(logo)} class:spin-enabled={station.spinLogo === true} class:is-playing={playing}/>
    <div class="frequency">{String(number).padStart(2, '0')}</div>
    <div class="station-name"><strong>{station.station}</strong></div>
    <div class="station-sub">{subtitle}</div>
  </div>
{/if}
