import type {DesktopSnapshot} from './desktop';

// Keep one IPC request in flight and one latest snapshot. Controls and metadata
// pass immediately; a moving clock alone needs at most one publication a second.
export class DesktopPublisher {
  private send:(snapshot:DesktopSnapshot)=>Promise<void>;
  private now:()=>number;
  private previous?:DesktopSnapshot;
  private sentAt=-Infinity;
  private latest?:DesktopSnapshot;
  private pending?:DesktopSnapshot;
  private force=false;
  private active?:Promise<void>;
  private retry?:ReturnType<typeof setTimeout>;
  private disposed=false;

  constructor(send:(snapshot:DesktopSnapshot)=>Promise<void>,now=()=>performance.now()) {
    this.send=send;this.now=now;
  }
  publish(snapshot:DesktopSnapshot,force=false):Promise<void> {
    if(this.disposed)return Promise.resolve();
    this.latest=this.pending={...snapshot};
    this.force ||= force;
    return this.active ??= this.drain();
  }
  private async drain() {
    // Let publish assign active before a synchronous sender can finish or fail.
    await Promise.resolve();
    try {
      while(this.pending&&!this.disposed) {
        const next=this.pending,force=this.force;
        this.pending=undefined;this.force=false;
        const previous=this.previous;
        const keys=Object.keys({...previous,...next}) as (keyof DesktopSnapshot)[];
        const changed=!previous||keys.some(key=>key!=='seconds'&&next[key]!==previous[key]);
        const clockDue=previous&&next.seconds!==previous.seconds&&this.now()-this.sentAt>=1000;
        if(!force&&!changed&&!clockDue)continue;
        try {
          await this.send(next);
          this.previous=next;this.sentAt=this.now();
          clearTimeout(this.retry);this.retry=undefined;
        } catch(error) {
          // A newer queued control still gets a chance after a failed request.
          if(this.pending)continue;
          if(!this.disposed&&!this.retry)this.retry=setTimeout(()=>{
            this.retry=undefined;
            if(this.latest)void this.publish(this.latest,true).catch(()=>{});
          },1000);
          throw error;
        }
      }
    } finally {this.active=undefined;}
  }
  dispose(){this.disposed=true;this.pending=undefined;clearTimeout(this.retry);}
}
