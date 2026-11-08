<script lang="ts">
  import {untrack} from 'svelte';
  import Dialog from './Dialog.svelte';
  import {saveGroup, deleteGroup, type Config, type Group} from './library';
  let {config,group,onsaved,oncancel}: {config: Config; group: Group|null; onsaved: (config:Config,id:string)=>void; oncancel:()=>void} = $props();
  const initial=untrack(()=>group);
  const id=initial?.id??crypto.randomUUID();
  let name=$state(initial?.name??'');
  let members=$state<string[]>([...(initial?.members??[])]);
  let busy=$state(false), error=$state(''), deleting=$state(false);
  function toggle(key:string){members=members.includes(key)?members.filter(m=>m!==key):[...members,key];}
  async function save(){busy=true;error='';try{onsaved(await saveGroup($state.snapshot({id,name,members})),id);}catch(e){error=String(e);}finally{busy=false;}}
  async function remove(){busy=true;error='';try{onsaved(await deleteGroup(id),'all');}catch(e){error=String(e);}finally{busy=false;}}
</script>

<Dialog title={initial?'Edit group':'Create group'} {oncancel} {busy} onsubmit={()=>void save()}>
  <fieldset disabled={busy}>
    <label>Group name<input required maxlength="200" bind:value={name}/></label>
    <p>Choose stations or individual tracks:</p>
    <div class="member-list">{#each config.custom as station}<details><summary><label><input type="checkbox" checked={members.includes(station.key)} onchange={()=>toggle(station.key)}/>{station.game?`${station.game} / `:''}{station.station}</label></summary>
      {#if station.mode==='tracks'}{#each station.tracks as track}<label class="track-choice"><input type="checkbox" checked={members.includes(`${station.key}|${track.path}`)} onchange={()=>toggle(`${station.key}|${track.path}`)}/>{track.name}</label>{/each}{/if}
    </details>{/each}</div>
    {#if initial}<button type="button" class="danger" onclick={()=>deleting=true}>Delete group</button>{/if}
    {#if deleting}<div class="notice"><p>Delete this group? Stations and audio files stay in your library.</p><button type="button" class="danger" onclick={remove}>Confirm delete</button></div>{/if}
  </fieldset>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#snippet footer()}<button type="button" onclick={oncancel} disabled={busy}>Cancel</button><button class="primary" disabled={busy||!name.trim()}>Save group</button>{/snippet}
</Dialog>
