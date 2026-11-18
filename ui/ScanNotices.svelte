<script lang="ts">
  import type {ScanNotices} from './library';
  import {displayPath} from './notifications';
  let {report,ondismiss}: {report:ScanNotices|null;ondismiss:()=>void}=$props();
  const count=$derived((report?.skippedFolders??0)+(report?.skippedEntries??0));
  const summary=$derived(report?[
    report.skippedFolders?`${report.skippedFolders} folder${report.skippedFolders===1?'':'s'}`:'',
    report.skippedEntries?`${report.skippedEntries} entr${report.skippedEntries===1?'y':'ies'}`:'',
  ].filter(Boolean).join(' and '):'');
</script>

{#if report && count}
  <div class="scan-notices" role="status">
    <p>{summary} could not be read.</p>
    <details><summary>View skipped items</summary><ul>{#each report.issues as issue}<li><code>{displayPath(issue.path)}</code> — {issue.reason}</li>{/each}</ul>
      {#if count>report.issues.length}<p>Showing the first {report.issues.length} notices.</p>{/if}
    </details>
    <button type="button" onclick={ondismiss}>Dismiss scan report</button>
  </div>
{/if}

<style>
  .scan-notices{border:1px solid var(--line,#365347);padding:12px;margin-block:12px;overflow-wrap:anywhere}
  ul{max-height:180px;overflow:auto;padding-left:22px}
  li{margin-block:8px}
</style>
