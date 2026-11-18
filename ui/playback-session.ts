import { NativePlayback } from './playback/native.ts';
import { connectMediaSession } from './media-session.ts';
import type { Playback, PlaybackState } from './playback/engine';
import type { AudioLevels, PlaybackStatus } from './library';
import type { DesktopAction, DesktopSnapshot } from './desktop';
export type SessionPlayer = Playback | NativePlayback;
interface Options {
    onState: (state: PlaybackState) => void;
    onAction: (action: DesktopAction) => void;
    group: () => string;
}
interface Factories {
    native: (options: {
        levels: AudioLevels;
        onState: Options['onState'];
    }) => SessionPlayer;
    loadWeb: () => Promise<{
        createWebPlayback: typeof import('./playback/web').createWebPlayback;
    }>;
    media: typeof connectMediaSession;
}
const defaults: Factories = {
    native: options => new NativePlayback(options),
    loadWeb: () => import('./playback/web'),
    media: connectMediaSession,
};
/** Own backend startup and cleanup, including cancellation during lazy loading. */
export class PlaybackSession {
    readonly ready: Promise<SessionPlayer | null>;
    private player: SessionPlayer | null = null;
    private media: ReturnType<typeof connectMediaSession> | undefined;
    private disposed = false;
    constructor(status: PlaybackStatus, options: Options, factories: Factories = defaults) {
        this.ready = this.start(status, options, factories);
    }
    private async start(status: PlaybackStatus, options: Options, factories: Factories) {
        if (status.error)
            throw new Error(status.error);
        const onState: Options['onState'] = state => { if (!this.disposed)
            options.onState(state); };
        try {
            if (status.backend === 'mpv') {
                this.player = factories.native({ levels: status.levels, onState });
            }
            else {
                const web = await factories.loadWeb();
                if (this.disposed)
                    return null;
                this.media = factories.media(action => { if (!this.disposed)
                    options.onAction(action); });
                this.player = web.createWebPlayback(onState, options.group, status.levels);
            }
            return this.player;
        }
        catch (error) {
            try {
                this.dispose();
            }
            catch {
                // Keep the startup error; dispose still releases the player.
            }
            throw error;
        }
    }
    updateMedia(snapshot: DesktopSnapshot) {
        if (!this.disposed)
            this.media?.update(snapshot);
    }
    dispose() {
        if (this.disposed)
            return;
        this.disposed = true;
        try {
            this.media?.dispose();
        }
        finally {
            this.player?.dispose();
        }
    }
}
