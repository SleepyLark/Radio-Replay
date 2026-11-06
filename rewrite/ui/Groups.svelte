<script lang="ts">
  import { desktop, saveGroup, deleteGroup, type Config, type Group } from './library';
  let {config,onsaved}:{config:Config;onsaved:(config:Config)=>void}=$props();
  let id=$state(''); let name=$state(''); let members=$state<string[]>([]); let busy=$state(false); let error=$state(''); let deleting=$state(false);
  const groups=$derived(config.groups.filter(g=>g && typeof g.id==='string' && typeof g.name==='string' && Array.isArray(g.members)));
  function edit(group?:Group) {id=group?.id??'';name=group?.name??'';members=[...(group?.members??[])];deleting=false;error='';}
  function toggle(key:string) {members=members.includes(key)?members.filter(m=>m!==key):[...members,key];}
  async function perform(action:()=>Promise<Config>) {busy=true;error='';try{onsaved(await action());edit();}catch(e){error=String(e);}finally{busy=false;}}
</script>
<section class="heading"><div><h1>Your groups</h1><p>Build a lineup from stations or individual music tracks.</p></div><button class="secondary" onclick={()=>edit()} disabled={busy}>New group</button></section>
<div class="actions">{#each groups as group}<button class="secondary" onclick={()=>edit(group)} disabled={busy}>{group.name}</button>{/each}</div>
<form class="editor" onsubmit={event=>{event.preventDefault();void perform(()=>saveGroup($state.snapshot({id:id||crypto.randomUUID(),name,members})));}}>
  <h2>{id?'Edit group':'Create group'}</h2>
  {#if error}<div class="error" role="alert">{error}</div>{/if}
  <fieldset disabled={busy||!desktop}>
    <label>Group name<input required maxlength="200" bind:value={name}/></label>
    {#each config.custom as station}<details><summary><label class="check"><input type="checkbox" checked={members.includes(station.key)} onchange={()=>toggle(station.key)}/>{station.station}</label></summary>
      {#if station.mode==='tracks'}{#each station.tracks as track}<label class="check track-choice"><input type="checkbox" checked={members.includes(`${station.key}|${track.path}`)} onchange={()=>toggle(`${station.key}|${track.path}`)}/>{track.name}</label>{/each}{/if}
    </details>{/each}
    <div class="actions editor-footer"><button class="primary" disabled={!name.trim()}>Save group</button>{#if id}<button class="danger" type="button" onclick={()=>deleting=true}>Delete group</button>{/if}</div>
    {#if deleting}<div class="notice"><p>Delete this group? Stations and media files stay in your library.</p><button class="danger" type="button" onclick={()=>perform(()=>deleteGroup(id))}>Confirm delete</button><button type="button" onclick={()=>deleting=false}>Keep group</button></div>{/if}
  </fieldset>
</form>
