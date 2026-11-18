import type { DesktopSnapshot } from './desktop';
import type { Station, TrackDetails } from './library';
import type { PlaybackState } from './playback/engine';
interface Context {
    playback: PlaybackState | null;
    station: Station | undefined;
    tags: TrackDetails['tags'] | undefined;
    textMode: string;
    displayTitle: string;
    art: string;
    banner: string;
    accent: string;
    canNavigate: boolean;
}
/** Build the shared mini-player/media snapshot from the current UI projection. */
export function desktopSnapshot(context: Context): DesktopSnapshot {
    const { playback, station, tags } = context;
    const title = playback?.bumper
        ? (tags?.title?.trim() ? `Bumper · ${tags.title.trim()}` : 'Bumper')
        : context.textMode === 'titles' ? context.displayTitle : (tags?.title || playback?.title || '');
    return {
        audioReady: playback !== null,
        station: station?.station ?? '',
        trackId: `${playback?.key ?? ''}\0${playback?.path ?? ''}`,
        network: station?.mode === 'network',
        title, artist: tags?.artist ?? '', album: tags?.album ?? '',
        art: context.art, banner: context.banner,
        bannerColor: typeof station?.bannerColor === 'string' ? station.bannerColor : '',
        accent: context.accent,
        playing: playback?.playing ?? false,
        hasStation: !!playback?.key && !playback.loading,
        canNavigate: context.canNavigate,
        seconds: playback?.seconds ?? 0, duration: playback?.duration ?? 0,
        volume: playback?.volume ?? .8, muted: playback?.muted ?? false,
    };
}
