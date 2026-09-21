<#ftl output_format="HTML" auto_esc=true>
<#macro content>
  <aside class="fabric-welcome-panel" aria-labelledby="fabric-welcome-title" data-template="welcome-panel">
    <span class="fabric-welcome-label">${realm.displayName!realm.name}</span>
    <h2 id="fabric-welcome-title">${msg("fabricWelcomeTitle")}</h2>
    <p>${msg("fabricWelcomeDescription")}</p>
    <div class="fabric-welcome-detail">
      <span class="fabric-welcome-number" aria-hidden="true">01</span>
      <div><strong>${msg("fabricAccountTitle")}</strong><p>${msg("fabricAccountDescription")}</p></div>
    </div>
    <div class="fabric-welcome-detail">
      <span class="fabric-welcome-number" aria-hidden="true">02</span>
      <div><strong>${msg("fabricHelpTitle")}</strong><p>${msg("fabricHelpDescription")}</p></div>
    </div>
  </aside>
</#macro>
