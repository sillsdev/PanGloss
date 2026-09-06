param(
	[ValidateSet('check', 'test')]
	[string]$Mode = 'check'
)

$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
$csproj = Join-Path $root 'XampleProjector.csproj'

# A .fwdata is plain XML with random guids and timestamps sprinkled through it, so a byte diff
# across two independent 'author' runs is meaningless until both are normalized the same way --
# known guids become their fixture id (readable AND still catches an id resolving to the wrong
# object), everything else collapses to fixed placeholders so only real structural differences
# survive the diff.
function Get-GuidToFixtureIdMap {
	param($AuthorResponse)
	$map = @{}
	foreach ($prop in $AuthorResponse.guidMap.PSObject.Properties) {
		$map[$prop.Value] = $prop.Name
	}
	return $map
}

# Container elements the .fwdata format writes for an unordered LCM collection (OC/RC) rather
# than an ordered sequence (OS/RS) -- measured against the pilot fixture's own PartOfSpeech
# (AffixSlotsOC), PhPhonemeSet (PhonemesOC, BoundaryMarkersOC), and MoInflAffMsa (SlotsRC) records.
# PrefixSlotsRS/SuffixSlotsRS/AlternateFormsOS are deliberately absent: their order is the exact
# thing this project's "Slot ordering" section and allomorph-ordering rule depend on, so sorting
# them here would hide a real regression instead of removing an artifact.
$script:UnorderedFwdataContainers = 'AffixSlots', 'Phonemes', 'BoundaryMarkers', 'Slots'

function Sort-UnorderedContainerChildren {
	param([string]$RecordText)
	foreach ($container in $script:UnorderedFwdataContainers) {
		$RecordText = [regex]::Replace($RecordText, "(?s)<$container>(.*?)</$container>", {
			param($m)
			$items = [regex]::Matches($m.Groups[1].Value, '<objsur[^/]*/>') | ForEach-Object { $_.Value } | Sort-Object
			"<$container>" + ($items -join '') + "</$container>"
		})
	}
	return $RecordText
}

function Get-NormalizedFwdataText {
	param([string]$Path, [hashtable]$GuidToFixtureId)
	$text = Get-Content -Raw -Path $Path
	foreach ($guid in $GuidToFixtureId.Keys) {
		$text = $text -replace [regex]::Escape($guid), $GuidToFixtureId[$guid]
	}
	$text = $text -replace '[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}', 'GUID'
	$text = $text -replace '\d{4}-\d{1,2}-\d{1,2} \d{1,2}:\d{2}:\d{2}(\.\d+)?', 'TIMESTAMP'

	# Measured: a .fwdata's top-level <rt> records are written sorted by the object's own guid --
	# sorting the pilot's 151 records by their pre-normalization guid reproduces the on-disk order
	# exactly. Blanking that guid removes the only key the order ever depended on, so two
	# independent runs keep the SAME records in a DIFFERENT order (and the same is true, one level
	# down, of each unordered collection's own <objsur> children); re-sorting both before comparing
	# removes that artifact instead of chasing it as a false failure.
	$firstRtIndex = $text.IndexOf('<rt ')
	if ($firstRtIndex -lt 0) { return $text }
	$header = $text.Substring(0, $firstRtIndex)
	$body = $text.Substring($firstRtIndex)
	$singleLine = [System.Text.RegularExpressions.RegexOptions]::Singleline
	# <rt .../> (self-closing, e.g. FsFeatureSystem) and <rt ...>...</rt> are both valid record
	# shapes; matching only the latter silently glues a self-closing record onto the NEXT record.
	$records = [regex]::Matches($body, '<rt\b[^>]*/>|<rt\b.*?</rt>', $singleLine) |
		ForEach-Object { Sort-UnorderedContainerChildren $_.Value } | Sort-Object
	return $header + ($records -join "`n")
}

# The direct text field a record's own content-derived label is read from, tried in this order --
# whichever field a record's CLASS actually exposes (never content), so the same class always
# picks the same field on both sides of any comparison. "Form" is deliberately absent: a phonetic
# shape is not reliably unique (two different affixes/roots can share the identical shape, and
# measured: the pilot fixture's own rules do), so a Form-bearing record (MoAffixAllomorph/
# MoStemAllomorph) is labeled through its owner instead (see the owner-based pass below).
$script:LabelDirectFieldNames = 'Name', 'Gloss', 'Representation', 'StringRepresentation', 'Abbreviation'

# Classes allowed to fall back to an own-content signature (below, once neither a direct field nor
# an owner works) when GrammarAuthor actually creates several of them targeting DIFFERENT things --
# NOT a blanket fallback: a project also carries scaffold objects FieldWorks itself populates
# (CmAnnotationDefn, CmPossibilityList, ...) that GrammarAuthor never touches and whose own content
# can legitimately differ between two independently created projects for reasons that have nothing
# to do with the grammar (list-membership ordering, locale-driven defaults). Measured: labeling
# those by content turned harmless, expected variation into a false witness-drift failure -- the
# allowlist keeps the new fallback scoped to the reviewer-demonstrated gap it exists to close.
$script:OwnContentLabelClasses = 'MoMorphAdhocProhib', 'MoAlloAdhocProhib'

function Get-XmlFieldText {
	param([System.Xml.Linq.XElement]$RecordElement, [string]$FieldName)
	$field = $RecordElement.Element([System.Xml.Linq.XName]$FieldName)
	if (-not $field) { return $null }
	$auni = $field.Descendants([System.Xml.Linq.XName]'AUni') | Select-Object -First 1
	if ($auni -and $auni.Value.Trim().Length -gt 0) { return $auni.Value.Trim() }
	$runText = (($field.Descendants([System.Xml.Linq.XName]'Run') | ForEach-Object { $_.Value }) -join '').Trim()
	if ($runText.Length -gt 0) { return $runText }
	if (-not $field.Elements()) {
		$direct = $field.Value.Trim()
		if ($direct.Length -gt 0) { return $direct }
	}
	return $null
}

# A deterministic, content-only tie-break key for two records that would otherwise receive the
# identical candidate label (e.g. two allomorphs owned by the same LexEntry, or two co-occurrence
# rules that share this project's one MoMorphData -- LibLCM's `MorphologicalDataOA` -- owner):
# every child element's own text, with any nested guid resolved to its OWN already-known label (or
# the blanket "GUID" placeholder, never the raw guid -- a raw guid would reintroduce exactly the
# random, run-specific ordering this whole function exists to remove). Never touches the record's
# own identity/owner attributes, which differ by construction and would make every signature
# trivially unique for the wrong reason. Also used (below) to derive a candidate FOR a record with
# no better option, from its own referenced content rather than an owner's.
function Get-RecordSignature {
	param([System.Xml.Linq.XElement]$RecordElement, [hashtable]$Records)
	$text = ($RecordElement.Elements() | ForEach-Object { $_.ToString() }) -join ''
	return [regex]::Replace($text, '[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}', {
		param($m)
		if ($Records.ContainsKey($m.Value) -and $Records[$m.Value].Label) { $Records[$m.Value].Label } else { 'GUID' }
	})
}

# Guids appearing in RecordElement's own content that name a KNOWN record (this project's own LCM
# object) which has not settled a label YET -- as opposed to one that will never get a label at
# all, which Get-RecordSignature already renders as the permanent "GUID" placeholder. Gates the
# own-content candidate below so it is only computed once every reference it contains is FINAL,
# never a same-round "GUID" placeholder that could make two genuinely different targets collide.
function Get-UnresolvedKnownReferenceGuids {
	param([System.Xml.Linq.XElement]$RecordElement, [hashtable]$Records)
	$text = ($RecordElement.Elements() | ForEach-Object { $_.ToString() }) -join ''
	return [regex]::Matches($text, '[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}') |
		ForEach-Object { $_.Value } |
		Where-Object { $Records.ContainsKey($_) -and -not $Records[$_].Label } |
		Select-Object -Unique
}

# Builds a guid -> content-derived label map from a SINGLE project's own .fwdata, independent of
# any external guidMap -- so two independently produced copies of the SAME grammar (e.g. a
# checked-in FieldWorks witness with no committed guidMap, and a freshly authored run) get
# comparable labels even though neither side's guids mean anything to the other. A record's own
# Name/Gloss/Representation/StringRepresentation/Abbreviation text becomes "{class}:{text}" (a
# reviewer-demonstrated gap: blanking every guid to the literal "GUID" makes "which slot serves
# this rule" and "which POS this rule requires" invisible, because two structurally-identical
# fragments differing only in THAT wiring then normalize byte-identically). A record with none of
# those fields (LexEntry; the allomorph that carries a rule's own shape) instead borrows its
# resolved owner's or owned child's label -- the one reverse relationship this needs is LexEntry's
# owned LexSense (its Gloss); every other reference target GrammarAuthor creates (PartOfSpeech,
# PhPhoneme, PhBdryMarker, PhNCSegments, ProdRestrict, MoInflAffixSlot, MoInflAffixTemplate)
# already carries a direct field. A record whose OWNER also has none of this AND whose class is on
# the narrow $script:OwnContentLabelClasses allowlist (MoMorphAdhocProhib/MoAlloAdhocProhib, owned
# by the never-Named MoMorphData singleton -- a second reviewer-demonstrated gap: two such rules
# differing only in WHICH morpheme/allomorph they target used to normalize identically) instead
# derives a candidate from its OWN referenced content (Get-RecordSignature), deferred via
# Get-UnresolvedKnownReferenceGuids until every reference it contains has itself settled, so the
# candidate reflects each target's FINAL label. See that allowlist's own comment for why this is NOT
# a blanket fallback for every unowned record. What a bare MSA POINTS AT gets a real label even
# though that record's own identity does not strictly need one -- it still gets one here (via its
# owner), which is what makes an ambiguous case (two allomorphs on the same entry) resolvable by
# content instead of refusing outright. A record this cannot label at all (see README's
# "content-derived labeling: known residual blind spot" for which, and why) falls through to
# Get-NormalizedFwdataText's existing blanket "GUID" blind, unchanged from before this function
# existed.
function Get-ContentDerivedGuidLabelMap {
	param([string]$Path, [string]$CheckExePath)

	$xdoc = [System.Xml.Linq.XDocument]::Load($Path)
	$records = @{}
	$childrenByOwner = @{}
	foreach ($rt in $xdoc.Root.Elements([System.Xml.Linq.XName]'rt')) {
		$guid = $rt.Attribute('guid').Value
		$class = $rt.Attribute('class').Value
		$ownerAttr = $rt.Attribute('ownerguid')
		$owner = if ($ownerAttr) { $ownerAttr.Value } else { $null }
		$records[$guid] = [pscustomobject]@{ Class = $class; OwnerGuid = $owner; Element = $rt; Label = $null }
		if ($owner) {
			if (-not $childrenByOwner.ContainsKey($owner)) { $childrenByOwner[$owner] = New-Object System.Collections.Generic.List[string] }
			$childrenByOwner[$owner].Add($guid)
		}
	}

	foreach ($guid in @($records.Keys)) {
		$rec = $records[$guid]
		foreach ($fieldName in $script:LabelDirectFieldNames) {
			$text = Get-XmlFieldText -RecordElement $rec.Element -FieldName $fieldName
			if ($text) { $rec.Label = "$($rec.Class):$text"; break }
		}
	}

	# Fixpoint: a record with no direct field of its own borrows its owned LexSense's Gloss (e.g.
	# LexEntry), its resolved owner's label (e.g. an allomorph or MSA, via its owning LexEntry), or
	# -- when neither applies -- a signature of its OWN referenced content once every reference in
	# it has settled (e.g. a co-occurrence rule, via the morpheme/allomorph it targets). Computed as
	# CANDIDATES across the whole pass, then committed together, so two records that would land on
	# the identical candidate are caught and tie-broken by content (Get-RecordSignature) rather than
	# one silently claiming the label first depending on enumeration order.
	$progress = $true
	while ($progress) {
		$progress = $false
		$candidates = @{}
		foreach ($guid in @($records.Keys)) {
			$rec = $records[$guid]
			if ($rec.Label) { continue }
			$kids = $childrenByOwner[$guid]
			$senseKid = if ($kids) { $kids | Where-Object { $records[$_].Class -eq 'LexSense' -and $records[$_].Label } | Select-Object -First 1 }
			if ($senseKid) {
				$suffix = $records[$senseKid].Label.Substring($records[$senseKid].Label.IndexOf(':') + 1)
				$candidates[$guid] = "$($rec.Class):$suffix"
			}
			elseif ($rec.OwnerGuid -and $records.ContainsKey($rec.OwnerGuid) -and $records[$rec.OwnerGuid].Label) {
				$candidates[$guid] = "$($rec.Class):$($records[$rec.OwnerGuid].Label)"
			}
			elseif (($script:OwnContentLabelClasses -contains $rec.Class) -and
				-not (Get-UnresolvedKnownReferenceGuids -RecordElement $rec.Element -Records $records)) {
				$signature = Get-RecordSignature -RecordElement $rec.Element -Records $records
				$candidates[$guid] = "$($rec.Class):$signature"
			}
		}
		if ($candidates.Count -eq 0) { break }
		$progress = $true
		foreach ($group in ($candidates.GetEnumerator() | Group-Object -Property Value)) {
			$members = @($group.Group.Name)
			if ($members.Count -eq 1) {
				$records[$members[0]].Label = $group.Name
				continue
			}
			$ordered = $members | Sort-Object -Property @{ Expression = { Get-RecordSignature -RecordElement $records[$_].Element -Records $records } }
			for ($i = 0; $i -lt $ordered.Count; $i++) {
				$records[$ordered[$i]].Label = "$($group.Name)#$i"
			}
		}
	}

	$labelMap = @{}
	foreach ($guid in $records.Keys) {
		if ($records[$guid].Label) { $labelMap[$guid] = $records[$guid].Label }
	}

	# Label uniqueness is PROVEN, not assumed: XampleProjector.exe's own check-label-uniqueness
	# command refuses (the same IdRegistry.Register guarded-insert GrammarParser uses to prove a
	# grammar.xml's ids/Names are document-global-unique) if two guids would render identically --
	# calling the existing enforcement rather than writing a second, independent uniqueness check.
	$labelsJsonPath = Join-Path ([System.IO.Path]::GetTempPath()) ("xample-projector-labels-" + [System.Guid]::NewGuid().ToString('N') + '.json')
	try {
		($labelMap | ConvertTo-Json -Depth 3) | Set-Content -Path $labelsJsonPath -Encoding utf8
		$checkOutput = & $CheckExePath check-label-uniqueness --labels $labelsJsonPath 2>&1
		if ($LASTEXITCODE -ne 0) {
			Write-Error "Get-ContentDerivedGuidLabelMap ($Path): content-derived labels are not unique -- $(($checkOutput | Out-String).Trim())"
			exit 1
		}
	}
	finally {
		Remove-Item -Path $labelsJsonPath -Force -ErrorAction SilentlyContinue
	}
	return $labelMap
}

# mutate/parse live proof helpers.

# The exact closed set of "morphotactic name" prefixes gram.txt/adctl.txt glue an hvo onto with no
# separator (e.g. "RootPOS106", "IrregInflForm79") -- every $sXxx literal in FieldWorks' own
# Src\Transforms\Application\XAmpleTemplateVariables.xsl. A prefix match still requires the next
# characters to be digits, so e.g. "InflClass" can never eat into "FromInflClass107"'s own "From".
$script:HvoIdentifierPrefixes = @(
	'IrregInflFormInSlot', 'IrregInflForm', 'DefaultExcpFeatures', 'FromExcpFeat', 'ToExcpFeat',
	'ExcpFeat', 'FromInflClass', 'ToInflClass', 'InflClass', 'FromMSFS', 'ToMSFS', 'MSEnvFS',
	'MSFS', 'InflectionFS', 'FromPOS', 'ToPOS', 'RootPOS', 'MSEnvPOS', 'CliticPOS', 'CFP',
	'StemName', 'ICA'
)

# Blind ONLY digit runs that are a known, XSL-confirmed hvo carrier -- never a raw \d+ blind (that
# also blinds a \maxp/\maxs/\maxi/\maxr/\maxn/\maxnull cap, a count, or a numeric gloss, none of
# which should ever compare equal after a phoneme deletion). Every shape below was found by diffing
# a live base project's adctl.txt/gram.txt/lex.txt against the SAME project's phoneme-deleted clone
# (every other line is byte-identical) and confirmed against the emitting XSL: bMorphnameIsMsaId=y
# (FxtM3ParserToXAmpleADCtl.xsl:39,217,268,284,305) and M3ModelExportServices.cs (every M3-dump
# "Id"/"dst" attribute is the LCM object's own Hvo) mean every one of these is a raw, renumberable
# hvo, never a value carrying independent meaning.
function Get-HvoBlindLines {
	param([string]$Path)
	$prefixAlternation = ($script:HvoIdentifierPrefixes -join '|')
	(Get-Content -Path $Path) | ForEach-Object {
		$line = $_
		# "<Prefix><hvo>" identifiers, e.g. "RootPOS106", "MSEnvPOS106", "IrregInflForm79".
		$line = [regex]::Replace($line, "\b($prefixAlternation)(\d+)\b", '${1}#')
		# "(<hvo>_<slotIndex>)" / "<<hvo>_<slotIndex> ...>" -- the index after "_" is a stable
		# 0..N-1 slot position (FxtM3ParserToToXAmpleGrammar.xsl), never renumbered; only the
		# leading hvo is.
		$line = [regex]::Replace($line, '\((\d+)(_\d+)\)', '(#$2)')
		$line = [regex]::Replace($line, '<(\d+)(_\d+) ', '<#$2 ')
		# "<... synCat>       = <hvo>" -- measured: the only two bare "= \d+$" lines in gram.txt,
		# both a POS's own hvo (same value as that POS's "RootPOS<hvo>" token elsewhere).
		$line = [regex]::Replace($line, '(synCat>\s*=\s*)(\d+)(\s*)$', '${1}#${3}')
		# "rootCat:<hvo>" / "fromCat:<hvo>" / "toCat:<hvo>" / "envCat:<hvo>".
		$line = [regex]::Replace($line, '((?:root|from|to|env)Cat:)(\d+)', '${1}#')
		# "rule {... template <hvo>}" -- the AffixTemplate's own hvo in a rule-name comment.
		$line = [regex]::Replace($line, '(template\s*)(\d+)(\})', '${1}#${3}')
		# lex.txt's "\lx <hvo>" / "\wc <hvo>" -- only when the WHOLE value is digits; a non-numeric
		# \wc (e.g. "root", "prefix", "circumSfx") is a literal category name, never an hvo, and
		# stays byte-compared.
		$line = [regex]::Replace($line, '^(\\lx )(\d+)\s*$', '${1}#')
		$line = [regex]::Replace($line, '^(\\wc )(\d+)\s*$', '${1}#')
		# lex.txt's "\a <form> {<hvo>}" -- the allomorph's own MoAffixAllomorph/MoStemAllomorph hvo
		# (FxtM3ParserToXAmpleLex.xsl's AlloForm template writes "{" + @Id + "}" verbatim).
		$line = [regex]::Replace($line, '^(\\a \S+ \{)(\d+)(\})', '${1}#${3}')
		$line
	}
}

# Deleting an unreferenced phoneme record shifts every LATER object's hvo within the same load
# session, so "unaffected by the phoneme deletion" is verified as byte-identical OR identical after
# blinding ONLY the hvo-bearing tokens above -- never a raw digit blind, which would also pass a
# corrupted cap or count (see Test-XampleFileCorruptionIsCaught below, which proves that directly).
function Test-XampleFilesEquivalentIgnoringHvoRenumbering {
	param([string]$BaseDir, [string]$CloneDir, [string]$Database, [string]$Label)
	foreach ($name in @('adctl.txt', 'gram.txt', 'lex.txt')) {
		$baseFile = Join-Path $BaseDir "$Database$name"
		$cloneFile = Join-Path $CloneDir "$Database$name"
		$baseRaw = Get-Content -Raw -Path $baseFile
		$cloneRaw = Get-Content -Raw -Path $cloneFile
		if ($baseRaw -ceq $cloneRaw) {
			Write-Host "  $name byte-identical (base vs $Label clone)."
			continue
		}
		$baseBlind = Get-HvoBlindLines -Path $baseFile
		$cloneBlind = Get-HvoBlindLines -Path $cloneFile
		if ($baseBlind.Count -ne $cloneBlind.Count) {
			Write-Error "$Label`: $name line count differs after hvo-blinding: base=$($baseBlind.Count) clone=$($cloneBlind.Count)"
			exit 1
		}
		for ($i = 0; $i -lt $baseBlind.Count; $i++) {
			if ($baseBlind[$i] -cne $cloneBlind[$i]) {
				Write-Error "$Label`: $name differs at line $($i + 1) beyond hvo renumbering.`n  base:  $($baseBlind[$i])`n  clone: $($cloneBlind[$i])"
				exit 1
			}
		}
		Write-Host "  $name identical to base except for hvo renumbering (base vs $Label clone; hvo-only-blind compare)."
	}
}

# Negative probe: the tightened compare must still CATCH a real regression. Flip one digit of a
# \maxp cap (never an hvo-bearing token -- see Get-HvoBlindLines) in a temp copy of the clone's own
# adctl.txt and assert the hvo-blind compare now reports a real difference at that exact line,
# rather than silently passing it the way a raw digit-blind (the pre-fix behavior) would have.
function Test-XampleFileCorruptionIsCaught {
	param([string]$CloneDir, [string]$Database, [string]$Label)
	$cloneAdctlPath = Join-Path $CloneDir "${Database}adctl.txt"
	$corruptDir = Join-Path ([System.IO.Path]::GetTempPath()) ("xample-projector-corrupt-probe-" + [System.Guid]::NewGuid().ToString('N'))
	New-Item -ItemType Directory -Path $corruptDir -Force | Out-Null
	try {
		$corruptAdctlPath = Join-Path $corruptDir "${Database}adctl.txt"
		$original = Get-Content -Path $cloneAdctlPath
		$maxpLine = $original | Where-Object { $_ -match '^\\maxp \d+$' } | Select-Object -First 1
		if (-not $maxpLine) {
			Write-Error "$Label`: corrupted-copy probe could not find a '\maxp <N>' line in $cloneAdctlPath to corrupt."
			exit 1
		}
		$corrupted = $original | ForEach-Object {
			if ($_ -eq $maxpLine) { $maxpLine -replace '\d+$', '3' } else { $_ }
		}
		Set-Content -Path $corruptAdctlPath -Value $corrupted -Encoding utf8
		if ((Get-Content -Raw -Path $corruptAdctlPath) -ceq (Get-Content -Raw -Path $cloneAdctlPath)) {
			Write-Error "$Label`: corrupted-copy probe failed to actually change anything -- '$maxpLine' not found verbatim?"
			exit 1
		}
		$origBlind = Get-HvoBlindLines -Path $cloneAdctlPath
		$corruptBlind = Get-HvoBlindLines -Path $corruptAdctlPath
		$foundDiff = $false
		for ($i = 0; $i -lt $origBlind.Count; $i++) {
			if ($origBlind[$i] -cne $corruptBlind[$i]) { $foundDiff = $true }
		}
		if (-not $foundDiff) {
			Write-Error "$Label`: corrupted-copy probe FAILED to catch a corrupted '\maxp' cap -- the hvo-only blind is over-broad."
			exit 1
		}
		Write-Host "  corrupted-copy probe OK ($Label): a corrupted '\maxp' cap ($maxpLine -> $($corrupted | Where-Object { $_ -match '^\\maxp \d+$' })) is still caught after the hvo-only blind."
	}
	finally {
		Remove-Item -Path $corruptDir -Recurse -Force -ErrorAction SilentlyContinue
	}
}

# Proves the witness-drift comparison can FAIL on a wiring-only difference, not just a
# literal-text one: swaps which of two structurally-identical MoInflAffMsa records' <Slots>
# reference points at which MoInflAffixSlot, then asserts content-derived labeling reports a real
# difference where the OLD blanket-"GUID" blind (both sides normalized with an empty map) reports
# none.
function Test-ContentDerivedLabelCatchesWiringSwap {
	param([string]$SourceFwdata, [string]$ExePath, [string]$Label)
	$swapDir = Join-Path ([System.IO.Path]::GetTempPath()) ("xample-projector-wiring-swap-probe-" + [System.Guid]::NewGuid().ToString('N'))
	New-Item -ItemType Directory -Path $swapDir -Force | Out-Null
	try {
		$swappedPath = Join-Path $swapDir 'swapped.fwdata'
		Copy-Item -Path $SourceFwdata -Destination $swappedPath -Force
		$text = Get-Content -Raw -Path $swappedPath

		$msaPattern = '<rt class="MoInflAffMsa"[^>]*>[\s\S]*?<Slots>\s*<objsur guid="([0-9a-fA-F-]{36})"[^/]*/>\s*</Slots>[\s\S]*?</rt>'
		$msaMatches = [regex]::Matches($text, $msaPattern)
		if ($msaMatches.Count -lt 2) {
			Write-Error "$Label`: wiring-swap probe needs at least 2 MoInflAffMsa records with a <Slots> reference in $SourceFwdata, found $($msaMatches.Count)."
			exit 1
		}
		$matchA = $msaMatches[0]
		$matchB = $msaMatches[1]
		$slotGuidA = $matchA.Groups[1].Value
		$slotGuidB = $matchB.Groups[1].Value
		if ($slotGuidA -eq $slotGuidB) {
			Write-Error "$Label`: wiring-swap probe's first two MoInflAffMsa records already target the same slot ($slotGuidA) -- cannot construct a real perturbation from them."
			exit 1
		}

		# Swap ONLY the two matched MSA records' own <Slots> reference -- a whole-document
		# find/replace of the two slot guids would also rename the SLOT records' own identity (and
		# every other reference to them, e.g. the owning POS's AffixSlots list), which is a
		# consistent rename (invisible to any comparison, content-derived or not), not a rewiring.
		# Splicing by match index/length, later match first, keeps the earlier match's index valid
		# and touches nothing outside these two records' own <Slots> element.
		$blockAReplacement = $matchA.Value -replace [regex]::Escape($slotGuidA), $slotGuidB
		$blockBReplacement = $matchB.Value -replace [regex]::Escape($slotGuidB), $slotGuidA
		$swappedText = $text.Remove($matchB.Index, $matchB.Length).Insert($matchB.Index, $blockBReplacement)
		$swappedText = $swappedText.Remove($matchA.Index, $matchA.Length).Insert($matchA.Index, $blockAReplacement)
		if ($swappedText -eq $text) {
			Write-Error "$Label`: wiring-swap probe failed to actually change anything."
			exit 1
		}
		Set-Content -Path $swappedPath -Value $swappedText -Encoding utf8 -NoNewline

		$genuineLabels = Get-ContentDerivedGuidLabelMap -Path $SourceFwdata -CheckExePath $ExePath
		$swappedLabels = Get-ContentDerivedGuidLabelMap -Path $swappedPath -CheckExePath $ExePath
		$genuineNormalized = Get-NormalizedFwdataText -Path $SourceFwdata -GuidToFixtureId $genuineLabels
		$swappedNormalized = Get-NormalizedFwdataText -Path $swappedPath -GuidToFixtureId $swappedLabels
		if ($genuineNormalized -eq $swappedNormalized) {
			Write-Error "$Label`: wiring-swap probe FAILED to catch a wiring-only perturbation (swapped slot $slotGuidA <-> $slotGuidB between two MoInflAffMsa records) -- content-derived labeling is over-broad."
			exit 1
		}
		Write-Host "  wiring-swap probe OK ($Label): reassigning which of two affix rules targets slot $slotGuidA vs $slotGuidB is caught by content-derived labeling (normalized text differs)."

		# The control: the OLD blanket-"GUID" blind (empty map both sides, this fix's BLOCKING
		# finding) must still be BLIND to the identical perturbation -- confirms the probe is
		# actually exercising the fixed code path, not a difference from some other cause.
		$genuineBlank = Get-NormalizedFwdataText -Path $SourceFwdata -GuidToFixtureId @{}
		$swappedBlank = Get-NormalizedFwdataText -Path $swappedPath -GuidToFixtureId @{}
		if ($genuineBlank -ne $swappedBlank) {
			Write-Error "$Label`: wiring-swap probe's own control failed -- the OLD blanket-GUID blind was expected to stay BLIND to this perturbation, but it reported a difference. Re-check the probe's premise."
			exit 1
		}
		Write-Host "  wiring-swap probe control OK ($Label): the OLD blanket-GUID blind (empty map both sides) is confirmed blind to the same perturbation content-derived labeling now catches."
	}
	finally {
		Remove-Item -Path $swapDir -Recurse -Force -ErrorAction SilentlyContinue
	}
}

# Same shape as Test-ContentDerivedLabelCatchesWiringSwap, for the OTHER reviewer-demonstrated gap:
# two structurally-identical MoAlloAdhocProhib records (owned by the never-Named MoMorphData
# singleton, so neither had ANY label before this fix) that differ only in which
# allomorph they exclude used to normalize to the same multiset -- Sort-Object over the record list
# cannot tell "A excludes X, B excludes Y" from "A excludes Y, B excludes X" once both records'
# OWN identity is blind. Swaps ONLY the <RestOfAllos> reference (leaving each record's
# <FirstAllomorph> -- its OTHER, unperturbed anchor -- untouched, exactly as the wiring-swap probe
# above touches only <Slots>), then asserts the same real-difference-vs-blind-control shape.
function Test-ContentDerivedLabelCatchesCoOccurrenceSwap {
	param([string]$SourceFwdata, [string]$ExePath, [string]$Label)
	$swapDir = Join-Path ([System.IO.Path]::GetTempPath()) ("xample-projector-cooccur-swap-probe-" + [System.Guid]::NewGuid().ToString('N'))
	New-Item -ItemType Directory -Path $swapDir -Force | Out-Null
	try {
		$swappedPath = Join-Path $swapDir 'swapped.fwdata'
		Copy-Item -Path $SourceFwdata -Destination $swappedPath -Force
		$text = Get-Content -Raw -Path $swappedPath

		$prohibPattern = '<rt class="MoAlloAdhocProhib"[^>]*>[\s\S]*?<RestOfAllos>\s*<objsur guid="([0-9a-fA-F-]{36})"[^/]*/>\s*</RestOfAllos>[\s\S]*?</rt>'
		$prohibMatches = [regex]::Matches($text, $prohibPattern)
		if ($prohibMatches.Count -lt 2) {
			Write-Error "$Label`: co-occurrence-swap probe needs at least 2 MoAlloAdhocProhib records with a <RestOfAllos> reference in $SourceFwdata, found $($prohibMatches.Count)."
			exit 1
		}
		$matchA = $prohibMatches[0]
		$matchB = $prohibMatches[1]
		$targetGuidA = $matchA.Groups[1].Value
		$targetGuidB = $matchB.Groups[1].Value
		if ($targetGuidA -eq $targetGuidB) {
			Write-Error "$Label`: co-occurrence-swap probe's first two MoAlloAdhocProhib records already exclude the same allomorph ($targetGuidA) -- cannot construct a real perturbation from them."
			exit 1
		}

		# Swap ONLY the two matched records' own <RestOfAllos> reference -- a whole-document
		# find/replace of the two target guids would also rename the ALLOMORPH records' own
		# identity (and every other reference to them), which is a consistent rename (invisible to
		# any comparison), not a rewiring. Splicing by match index/length, later match first, keeps
		# the earlier match's index valid and touches nothing outside these two records' own
		# <RestOfAllos> element.
		$blockAReplacement = $matchA.Value -replace [regex]::Escape($targetGuidA), $targetGuidB
		$blockBReplacement = $matchB.Value -replace [regex]::Escape($targetGuidB), $targetGuidA
		$swappedText = $text.Remove($matchB.Index, $matchB.Length).Insert($matchB.Index, $blockBReplacement)
		$swappedText = $swappedText.Remove($matchA.Index, $matchA.Length).Insert($matchA.Index, $blockAReplacement)
		if ($swappedText -eq $text) {
			Write-Error "$Label`: co-occurrence-swap probe failed to actually change anything."
			exit 1
		}
		Set-Content -Path $swappedPath -Value $swappedText -Encoding utf8 -NoNewline

		$genuineLabels = Get-ContentDerivedGuidLabelMap -Path $SourceFwdata -CheckExePath $ExePath
		$swappedLabels = Get-ContentDerivedGuidLabelMap -Path $swappedPath -CheckExePath $ExePath
		$genuineNormalized = Get-NormalizedFwdataText -Path $SourceFwdata -GuidToFixtureId $genuineLabels
		$swappedNormalized = Get-NormalizedFwdataText -Path $swappedPath -GuidToFixtureId $swappedLabels
		if ($genuineNormalized -eq $swappedNormalized) {
			Write-Error "$Label`: co-occurrence-swap probe FAILED to catch a wiring-only perturbation (swapped excluded allomorph $targetGuidA <-> $targetGuidB between two MoAlloAdhocProhib records) -- content-derived labeling is over-broad."
			exit 1
		}
		Write-Host "  co-occurrence-swap probe OK ($Label): reassigning which of two co-occurrence rules excludes allomorph $targetGuidA vs $targetGuidB is caught by content-derived labeling (normalized text differs)."

		# The control: the OLD blanket-"GUID" blind (empty map both sides) must still be BLIND to
		# the identical perturbation -- confirms the probe is actually exercising the fixed code
		# path, not a difference from some other cause.
		$genuineBlank = Get-NormalizedFwdataText -Path $SourceFwdata -GuidToFixtureId @{}
		$swappedBlank = Get-NormalizedFwdataText -Path $swappedPath -GuidToFixtureId @{}
		if ($genuineBlank -ne $swappedBlank) {
			Write-Error "$Label`: co-occurrence-swap probe's own control failed -- the OLD blanket-GUID blind was expected to stay BLIND to this perturbation, but it reported a difference. Re-check the probe's premise."
			exit 1
		}
		Write-Host "  co-occurrence-swap probe control OK ($Label): the OLD blanket-GUID blind (empty map both sides) is confirmed blind to the same perturbation content-derived labeling now catches."
	}
	finally {
		Remove-Item -Path $swapDir -Recurse -Force -ErrorAction SilentlyContinue
	}
}

# The shared shape of every exit-code probe in this file: run one projector subcommand, capture
# combined stdout+stderr, and assert BOTH the exit code and that the output names every expected
# substring (a RAW -notmatch pattern, exactly as each call site used before this was extracted --
# not auto-escaped, since at least one caller relies on that to match a literal regex fragment) --
# refusing loudly the moment either check fails. Returns the captured text so a caller can layer a
# probe-specific follow-up check (e.g. "no .fwdata left on disk") on top.
function Assert-ProjectorExit {
	param(
		[Parameter(Mandatory)][string]$ExePath,
		[Parameter(Mandatory)][string[]]$Arguments,
		[Parameter(Mandatory)][int]$ExpectedExitCode,
		[string[]]$ExpectedSubstrings = @(),
		[Parameter(Mandatory)][string]$Label
	)
	$output = & $ExePath @Arguments 2>&1
	$exitCode = $LASTEXITCODE
	$text = ($output | Out-String)
	if ($exitCode -ne $ExpectedExitCode) {
		Write-Error "$Label`: expected exit $ExpectedExitCode, got $exitCode. Output:`n$text"
		exit 1
	}
	foreach ($substring in $ExpectedSubstrings) {
		if ($text -notmatch $substring) {
			Write-Error "$Label`: exit was $ExpectedExitCode but output does not name '$substring'. Output:`n$text"
			exit 1
		}
	}
	return $text
}

function Assert-HcXmlLacksSegmentDefinition {
	param([string]$BaseHcXmlPath, [string]$CloneHcXmlPath, [string]$Representation, [string]$Label)
	$pattern = "(?s)<SegmentDefinition[^>]*>.*?<Representation>$([regex]::Escape($Representation))</Representation>.*?</SegmentDefinition>"
	$baseText = Get-Content -Raw -Path $BaseHcXmlPath
	$cloneText = Get-Content -Raw -Path $CloneHcXmlPath
	if ($baseText -notmatch $pattern) {
		Write-Error "$Label`: base hc.xml unexpectedly has no SegmentDefinition for '$Representation' -- cannot prove the clone dropped it."
		exit 1
	}
	if ($cloneText -match $pattern) {
		Write-Error "$Label`: clone hc.xml still contains a SegmentDefinition for '$Representation'."
		exit 1
	}
	Write-Host "  clone hc.xml correctly lacks the '$Representation' SegmentDefinition present in the base hc.xml ($Label)."
}

# msaGuid is a real LCM guid (ParseCommand resolves it via a live LcmCache), stable across a
# file-copy clone even though the raw hvo XAmple's XML embeds is only stable within one cache
# session -- so a per-analysis signature built from msaGuids is comparable between the base
# project and a phoneme-deleted clone even though their two parse runs open separate sessions.
function Get-AnalysisSignatures {
	param($ParseResponse, [string]$Word)
	$wordEntry = $ParseResponse.words | Where-Object { $_.word -eq $Word }
	if (-not $wordEntry) { return @() }
	return $wordEntry.analyses | ForEach-Object { ($_.morphemes | ForEach-Object { $_.msaGuid }) -join '+' } | Sort-Object
}

# phonology-mutations.yaml (machine\conformance\PROTOCOL.md section 10) is a small, fixed-shape
# manifest; a bespoke parser matches its exact shape rather than pulling in a general YAML reader
# (the Rust side owns YAML later).
function ConvertFrom-PhonologyMutationsYaml {
	param([string]$Path)
	$text = Get-Content -Raw -Path $Path

	$versionMatch = [regex]::Match($text, '(?m)^version:\s*(\d+)\s*$')
	if (-not $versionMatch.Success) { throw "phonology-mutations.yaml ($Path): no 'version:' line found" }
	$manifestVersion = [int]$versionMatch.Groups[1].Value
	if ($manifestVersion -ne 1) {
		throw "phonology-mutations.yaml ($Path): version $manifestVersion is not supported (this build only supports version 1)"
	}
	$shaMatch = [regex]::Match($text, '(?m)^base_sha256:\s*([0-9a-fA-F]{64})\s*$')
	if (-not $shaMatch.Success) { throw "phonology-mutations.yaml ($Path): no 64-hex 'base_sha256:' line found" }

	$cases = @()
	$caseBlocks = [regex]::Split($text, '(?m)^\s*- id:\s*') | Select-Object -Skip 1
	foreach ($block in $caseBlocks) {
		$caseId = [regex]::Match($block, '^(\S+)').Groups[1].Value

		$removePhonemeMatch = [regex]::Match($block,
			'remove_phoneme:\s*\r?\n\s*guid:\s*(\S+)\s*\r?\n\s*assert_representations:\s*\[([^\]]*)\]\s*\r?\n\s*require_unreferenced:\s*(true|false)')
		$removeAllMatch = [regex]::Match($block, 'remove_all_phonemes:\s*\r?\n\s*require_unreferenced:\s*(true|false)')
		if ($removePhonemeMatch.Success) {
			$assertReps = @($removePhonemeMatch.Groups[2].Value -split ',' | ForEach-Object { $_.Trim() } | Where-Object { $_ -ne '' })
			$operation = @{
				op                    = 'remove_phoneme'
				guid                  = $removePhonemeMatch.Groups[1].Value
				assertRepresentations = $assertReps
				requireUnreferenced   = [bool]::Parse($removePhonemeMatch.Groups[3].Value)
			}
		}
		elseif ($removeAllMatch.Success) {
			$operation = @{ op = 'remove_all_phonemes'; requireUnreferenced = [bool]::Parse($removeAllMatch.Groups[1].Value) }
		}
		else {
			throw "phonology-mutations.yaml ($Path): case '$caseId' has no recognized operation (v1 vocabulary: remove_phoneme, remove_all_phonemes)"
		}

		$xampleMatch = [regex]::Match($block, 'xample_projection:\s*(\S+)')
		$hcMatch = [regex]::Match($block, 'hc_analyses:\s*(\S+)')
		$segmentsMatch = [regex]::Match($block, 'inferred_segments:\s*\[([^\]]*)\]')
		if (-not $xampleMatch.Success -or -not $hcMatch.Success -or -not $segmentsMatch.Success) {
			throw "phonology-mutations.yaml ($Path): case '$caseId' is missing an expect.{xample_projection,hc_analyses,inferred_segments} field"
		}
		# v1's only defined expect value -- a manifest naming anything else must refuse, not be
		# silently treated as this build's own "same_as_base" checks.
		if ($xampleMatch.Groups[1].Value -ne 'same_as_base' -or $hcMatch.Groups[1].Value -ne 'same_as_base') {
			throw "phonology-mutations.yaml ($Path): case '$caseId' has an expect value outside the v1 vocabulary (only 'same_as_base' is defined)"
		}

		$cases += [pscustomobject]@{
			id                     = $caseId
			operation              = $operation
			expectInferredSegments = @($segmentsMatch.Groups[1].Value -split ',' | ForEach-Object { $_.Trim() } | Where-Object { $_ -ne '' })
		}
	}
	if ($cases.Count -eq 0) { throw "phonology-mutations.yaml ($Path): no cases found" }

	return [pscustomobject]@{
		version    = $manifestVersion
		baseSha256 = $shaMatch.Groups[1].Value.ToLowerInvariant()
		cases      = $cases
	}
}

function Get-MutationRequestFromManifestCase {
	param($Case, [string]$BaseSha256)
	$op = $Case.operation
	$operationObj = if ($op.op -eq 'remove_phoneme') {
		@{ op = 'remove_phoneme'; guid = $op.guid; assertRepresentations = @($op.assertRepresentations); requireUnreferenced = $op.requireUnreferenced }
	}
	else {
		@{ op = 'remove_all_phonemes'; requireUnreferenced = $op.requireUnreferenced }
	}
	return @{
		schemaVersion = 1
		caseId        = $Case.id
		baseSha256    = $BaseSha256
		operations    = @($operationObj)
	}
}

function Find-MSBuild {
	$vswhere = 'C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe'
	if (Test-Path $vswhere) {
		$vsPath = & $vswhere -latest -requires Microsoft.Component.MSBuild -property installationPath 2>$null
		if ($vsPath) {
			$msbuild = Join-Path $vsPath 'MSBuild\Current\Bin\MSBuild.exe'
			if (Test-Path $msbuild) { return $msbuild }
		}
	}
	return $null
}

function Invoke-Build {
	param([string[]]$ExtraArgs = @())
	$msbuild = Find-MSBuild
	if ($msbuild) {
		Write-Host "Building with MSBuild: $msbuild"
		& $msbuild $csproj /restore /nologo /verbosity:minimal @ExtraArgs | Out-Host
		return $LASTEXITCODE
	}
	Write-Host 'MSBuild (vswhere) not found; falling back to dotnet build.'
	if (-not (Get-Command dotnet -ErrorAction SilentlyContinue)) {
		Write-Error 'Neither MSBuild nor the dotnet SDK is available. Install one of them.'
		return 1
	}
	& dotnet build $csproj --nologo @ExtraArgs | Out-Host
	return $LASTEXITCODE
}

$exePath = Join-Path $root 'bin\Debug\XampleProjector.exe'

$buildExit = Invoke-Build
if ($buildExit -ne 0) {
	Write-Error "Build failed (exit $buildExit)."
	exit $buildExit
}
Write-Host 'Build OK.'

if ($Mode -eq 'check') {
	exit 0
}

# --- Mode 'test' ---

if (-not (Test-Path $exePath)) {
	Write-Error "Build reported success but $exePath is missing."
	exit 1
}

$capturedResponse = Join-Path $root 'testdata\captured-response.json'
Write-Host "Running --validate-capture against $capturedResponse"
& $exePath --validate-capture $capturedResponse
if ($LASTEXITCODE -ne 0) {
	Write-Error "--validate-capture failed on the checked-in capture (exit $LASTEXITCODE)."
	exit $LASTEXITCODE
}
Write-Host '--validate-capture OK.'

$capturedInspectResponse = Join-Path $root 'testdata\captured-inspect-response.json'
Write-Host "Running --validate-capture against $capturedInspectResponse"
& $exePath --validate-capture $capturedInspectResponse
if ($LASTEXITCODE -ne 0) {
	Write-Error "--validate-capture failed on the checked-in inspect capture (exit $LASTEXITCODE)."
	exit $LASTEXITCODE
}
Write-Host '--validate-capture (inspect) OK.'

$capturedAuthorResponse = Join-Path $root 'testdata\captured-author-response.json'
Write-Host "Running --validate-capture against $capturedAuthorResponse"
& $exePath --validate-capture $capturedAuthorResponse
if ($LASTEXITCODE -ne 0) {
	Write-Error "--validate-capture failed on the checked-in author capture (exit $LASTEXITCODE)."
	exit $LASTEXITCODE
}
Write-Host '--validate-capture (author) OK.'

# --- phonology-mutations.yaml version-refusal probe: needs no FieldWorks/exe at all, so it runs
#     unconditionally rather than only when a live witness happens to be reachable. ---
$badVersionManifestPath = Join-Path ([System.IO.Path]::GetTempPath()) ("xample-projector-bad-version-manifest-" + [System.Guid]::NewGuid().ToString('N') + '.yaml')
Set-Content -Path $badVersionManifestPath -Value "version: 2`nbase_sha256: $('0' * 64)`n" -Encoding utf8
try {
	$badVersionThrew = $false
	$badVersionMessage = $null
	try {
		ConvertFrom-PhonologyMutationsYaml -Path $badVersionManifestPath | Out-Null
	}
	catch {
		$badVersionThrew = $true
		$badVersionMessage = $_.Exception.Message
	}
	if (-not $badVersionThrew) {
		Write-Error "phonology-mutations.yaml version-refusal probe: version 2 was accepted with no error."
		exit 1
	}
	if ($badVersionMessage -notmatch 'version 2' -or $badVersionMessage -notmatch 'version 1') {
		Write-Error "phonology-mutations.yaml version-refusal probe: refused, but the message does not name both the version found and the version supported. Message:`n$badVersionMessage"
		exit 1
	}
	Write-Host "phonology-mutations.yaml version-refusal probe OK: version 2 refused -- $badVersionMessage"
}
finally {
	Remove-Item -Path $badVersionManifestPath -Force -ErrorAction SilentlyContinue
}

$fieldWorksDir = $env:PANGLOSS_FIELDWORKS_DIR
if ([string]::IsNullOrEmpty($fieldWorksDir)) { $fieldWorksDir = 'C:\Program Files\SIL\FieldWorks 9' }
$fwProjectsDir = $env:PANGLOSS_FW_PROJECTS_DIR
if ([string]::IsNullOrEmpty($fwProjectsDir)) { $fwProjectsDir = 'C:\Users\johnm\Documents\repos\FieldWorks\DistFiles\Projects' }
$senaSource = Join-Path $fwProjectsDir 'Sena 3'
$senaFwdata = Join-Path $senaSource 'Sena 3.fwdata'

if (-not (Test-Path $fieldWorksDir)) {
	Write-Host "SKIPPED (live run): FieldWorks install not found at $fieldWorksDir"
	exit 0
}

if (-not (Test-Path $senaFwdata)) {
	Write-Host "SKIPPED (Sena3 project/inspect live tests): sample project not found at $senaFwdata"
}
else {
$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("xample-projector-test-" + [System.Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
try {
	$projectCopy = Join-Path $tempRoot 'Sena 3'
	Write-Host "Copying $senaSource -> $projectCopy"
	Copy-Item -Path $senaSource -Destination $projectCopy -Recurse -Force

	$outDir = Join-Path $tempRoot 'out'
	New-Item -ItemType Directory -Path $outDir -Force | Out-Null

	$projectFwdata = Join-Path $projectCopy 'Sena 3.fwdata'
	Write-Host "Running: $exePath project --project `"$projectFwdata`" --out-dir `"$outDir`" --database Sena3"
	& $exePath project --project $projectFwdata --out-dir $outDir --database Sena3
	$liveExit = $LASTEXITCODE
	if ($liveExit -ne 0) {
		Write-Error "Live 'project' run failed (exit $liveExit)."
		exit $liveExit
	}

	$expectedFiles = @('Sena3.hc.xml', 'Sena3adctl.txt', 'Sena3gram.txt', 'Sena3lex.txt', 'Sena3XAmpleWordGrammarDebugger.xsl')
	foreach ($name in $expectedFiles) {
		$path = Join-Path $outDir $name
		if (-not (Test-Path $path)) {
			Write-Error "Live run did not produce expected file: $path"
			exit 1
		}
		$len = (Get-Item $path).Length
		if ($len -le 0) {
			Write-Error "Live run produced an empty file: $path"
			exit 1
		}
		Write-Host ("  {0} ({1} bytes)" -f $name, $len)
	}

	$responseJson = Join-Path $outDir 'response.json'
	if (-not (Test-Path $responseJson)) {
		Write-Error "Live run did not produce response.json at $responseJson"
		exit 1
	}

	Write-Host "Validating the live-run capture: $responseJson"
	& $exePath --validate-capture $responseJson
	if ($LASTEXITCODE -ne 0) {
		Write-Error "--validate-capture failed on the live-run response.json (exit $LASTEXITCODE)."
		exit $LASTEXITCODE
	}
	Write-Host 'Live run OK.'

	# --- Probe: 'inspect' produces at least one phoneme, each with a real guid ---
	$inspectResponse = Join-Path $tempRoot 'inspect-response.json'
	Write-Host "Running: $exePath inspect --project `"$projectFwdata`" --out `"$inspectResponse`""
	& $exePath inspect --project $projectFwdata --out $inspectResponse
	$inspectExit = $LASTEXITCODE
	if ($inspectExit -ne 0) {
		Write-Error "Live 'inspect' run failed (exit $inspectExit)."
		exit $inspectExit
	}
	$inspectJson = Get-Content $inspectResponse -Raw | ConvertFrom-Json
	$phonemeCount = $inspectJson.phonemes.Count
	if ($phonemeCount -le 0) {
		Write-Error "Live 'inspect' run reported zero phonemes."
		exit 1
	}
	foreach ($phoneme in $inspectJson.phonemes) {
		if ($null -eq $phoneme.guid -or $phoneme.guid.Length -ne 36) {
			Write-Error "Live 'inspect' run reported a phoneme with a malformed guid: '$($phoneme.guid)'"
			exit 1
		}
	}
	Write-Host "Live 'inspect' probe OK: $phonemeCount phoneme(s), all with 36-char guids."

	# --- Probe: two consecutive 'inspect' runs over the same project produce byte-identical JSON
	#     (pins InspectCommand's guid sort as the thing that makes this true) ---
	$inspectResponse2 = Join-Path $tempRoot 'inspect-response-2.json'
	& $exePath inspect --project $projectFwdata --out $inspectResponse2
	if ($LASTEXITCODE -ne 0) {
		Write-Error "Second live 'inspect' run (determinism probe) failed (exit $LASTEXITCODE)."
		exit $LASTEXITCODE
	}
	$inspectText1 = Get-Content $inspectResponse -Raw
	$inspectText2 = Get-Content $inspectResponse2 -Raw
	if ($inspectText1 -ne $inspectText2) {
		Write-Error "Determinism probe: two consecutive 'inspect' runs produced different JSON."
		exit 1
	}
	Write-Host "Live 'inspect' determinism probe OK: two runs produced byte-identical JSON ($($inspectText1.Length) bytes)."

	# --- Probe: two consecutive 'project' runs over the same copy agree exactly on every entry
	#     marked deterministic:true, and the sole deterministic:false entry is the GAFAWS OUT file
	#     (pins determinism by EFFECT: same sha256 across independent runs, not by the label alone) ---
	$outDir2 = Join-Path $tempRoot 'out2'
	New-Item -ItemType Directory -Path $outDir2 -Force | Out-Null
	Write-Host "Running second 'project' pass for determinism comparison: $exePath project --project `"$projectFwdata`" --out-dir `"$outDir2`" --database Sena3"
	& $exePath project --project $projectFwdata --out-dir $outDir2 --database Sena3 | Out-Null
	if ($LASTEXITCODE -ne 0) {
		Write-Error "Second 'project' run (determinism probe) failed (exit $LASTEXITCODE)."
		exit $LASTEXITCODE
	}

	$response1 = Get-Content $responseJson -Raw | ConvertFrom-Json
	$response2 = Get-Content (Join-Path $outDir2 'response.json') -Raw | ConvertFrom-Json

	if ($response1.generated.Count -ne $response2.generated.Count) {
		Write-Error "Determinism probe: run 1 produced $($response1.generated.Count) generated file(s), run 2 produced $($response2.generated.Count)."
		exit 1
	}
	$nonDeterministicCount = 0
	foreach ($entry1 in $response1.generated) {
		$entry2 = $response2.generated | Where-Object { $_.path -eq $entry1.path }
		if (-not $entry2) {
			Write-Error "Determinism probe: entry '$($entry1.path)' present in run 1 but not run 2."
			exit 1
		}
		if ($entry1.deterministic) {
			if ($entry1.sha256 -ne $entry2.sha256) {
				Write-Error "Determinism probe: '$($entry1.path)' is marked deterministic but sha256 differs between runs ($($entry1.sha256) vs $($entry2.sha256))."
				exit 1
			}
		}
		else {
			$nonDeterministicCount++
			if ($entry1.path -notmatch '^OUT.*gafawsData\.xml$') {
				Write-Error "Determinism probe: the entry marked non-deterministic ('$($entry1.path)') is not the GAFAWS OUT file."
				exit 1
			}
		}
	}
	if ($nonDeterministicCount -ne 1) {
		Write-Error "Determinism probe: expected exactly 1 entry marked deterministic:false, found $nonDeterministicCount."
		exit 1
	}
	Write-Host "Determinism probe OK: $($response1.generated.Count) generated file(s) compared across two runs; exactly 1 correctly marked non-deterministic (the GAFAWS OUT file); every other entry byte-identical."

	# --- Probe: an output-dir collision surfaces as ProjectionException -> exit 5, naming the step ---
	$collisionOutDir = Join-Path $tempRoot 'out-collision'
	New-Item -ItemType Directory -Path $collisionOutDir -Force | Out-Null
	$collisionPath = Join-Path $collisionOutDir 'Sena3adctl.txt'
	New-Item -ItemType Directory -Path $collisionPath -Force | Out-Null
	Write-Host "Running (expected to fail): $exePath project --project `"$projectFwdata`" --out-dir `"$collisionOutDir`" --database Sena3 (Sena3adctl.txt pre-created as a directory)"
	Assert-ProjectorExit -ExePath $exePath -Label 'Output-dir collision probe' -ExpectedExitCode 5 -ExpectedSubstrings 'adctl' -Arguments @(
		'project', '--project', $projectFwdata, '--out-dir', $collisionOutDir, '--database', 'Sena3'
	) | Out-Null
	Write-Host "Output-dir collision probe OK: exit 5, output names 'adctl'."
}
finally {
	Remove-Item -Path $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
}
}

# --- refusal-before-project-exists probes: GrammarParser.Parse runs before AuthorSession.Run ever
#     creates a .fwdata, so these need FieldWorks (for the pin check in Program.Main) but NEITHER
#     the machine submodule NOR Sena 3 -- both grammars are tiny inline fixtures. ---
$parserRefusalTempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("xample-projector-parser-refusal-" + [System.Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $parserRefusalTempRoot -Force | Out-Null
try {
	# MorphemeCoOccurrenceRule referencing an unknown id used to refuse only from inside
	# GrammarAuthor.CreateCoOccurrenceRules, after AuthorSession.Run had already created and locked
	# a real .fwdata. GrammarParser.Parse now refuses it before any project exists.
	$unknownRefGrammarXml = @'
<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE HermitCrabInput SYSTEM "HermitCrabInput.dtd">
<HermitCrabInput>
  <Language>
    <Name>UnknownCoOccurrenceRefProbe</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered">
        <Name>Main</Name>
        <LexicalEntries>
          <LexicalEntry id="eK" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aK"><PhoneticShape>k</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>K</MorphemeId>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
    <MorphemeCoOccurrenceRules>
      <MorphemeCoOccurrenceRule type="exclude" primaryMorpheme="eK" otherMorphemes="doesNotExist" adjacency="anywhere" />
    </MorphemeCoOccurrenceRules>
  </Language>
</HermitCrabInput>
'@
	$unknownRefGrammarPath = Join-Path $parserRefusalTempRoot 'unknown-ref.grammar.xml'
	Set-Content -Path $unknownRefGrammarPath -Value $unknownRefGrammarXml -Encoding utf8
	$unknownRefOutDir = Join-Path $parserRefusalTempRoot 'unknown-ref-out'
	Assert-ProjectorExit -ExePath $exePath -Label 'Unknown co-occurrence id refusal probe' -ExpectedExitCode 7 -ExpectedSubstrings 'doesNotExist' -Arguments @(
		'author', '--grammar', $unknownRefGrammarPath, '--out-dir', $unknownRefOutDir, '--name', 'UnknownRef'
	) | Out-Null
	$unknownRefFwdata = Join-Path $unknownRefOutDir 'UnknownRef\UnknownRef.fwdata'
	if (Test-Path $unknownRefFwdata) {
		Write-Error "Unknown co-occurrence id refusal probe: exit was 7 but a .fwdata was left on disk at $unknownRefFwdata."
		exit 1
	}
	Write-Host "Unknown co-occurrence id refusal probe OK: exit 7, output names 'doesNotExist', no .fwdata on disk."

	# ids are document-global per the DTD (XML ID type), but DtdProcessing.Ignore means nothing
	# enforced that -- a PartOfSpeech and a LexicalEntry sharing one id used to silently collide in
	# AuthorResult.GuidMap. GrammarParser.Parse now refuses the reused id up front.
	$duplicateIdGrammarXml = @'
<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE HermitCrabInput SYSTEM "HermitCrabInput.dtd">
<HermitCrabInput>
  <Language>
    <Name>DuplicateIdProbe</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="dup1"><Name>v</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered">
        <Name>Main</Name>
        <LexicalEntries>
          <LexicalEntry id="dup1" partOfSpeech="dup1">
            <Allomorphs><Allomorph id="aK"><PhoneticShape>k</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>K</MorphemeId>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
'@
	$duplicateIdGrammarPath = Join-Path $parserRefusalTempRoot 'duplicate-id.grammar.xml'
	Set-Content -Path $duplicateIdGrammarPath -Value $duplicateIdGrammarXml -Encoding utf8
	$duplicateIdOutDir = Join-Path $parserRefusalTempRoot 'duplicate-id-out'
	Assert-ProjectorExit -ExePath $exePath -Label 'Duplicate id refusal probe' -ExpectedExitCode 7 -ExpectedSubstrings 'dup1' -Arguments @(
		'author', '--grammar', $duplicateIdGrammarPath, '--out-dir', $duplicateIdOutDir, '--name', 'DuplicateId'
	) | Out-Null
	$duplicateIdFwdata = Join-Path $duplicateIdOutDir 'DuplicateId\DuplicateId.fwdata'
	if (Test-Path $duplicateIdFwdata) {
		Write-Error "Duplicate id refusal probe: exit was 7 but a .fwdata was left on disk at $duplicateIdFwdata."
		exit 1
	}
	Write-Host "Duplicate id refusal probe OK: exit 7, output names 'dup1', no .fwdata on disk."

	# GrammarParser.RegisterId used to track only DTD `id` attributes, but AuthorResult.GuidMap is
	# also keyed by AffixTemplate/Slot Name text -- two AffixTemplates each with a slot named "Root"
	# passed Parse and only then threw mid-Author, after a real .fwdata already existed.
	# GrammarParser.Parse now registers Name text in the same seenIds space, so this refuses before
	# any project exists.
	$duplicateSlotNameGrammarXml = @'
<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE HermitCrabInput SYSTEM "HermitCrabInput.dtd">
<HermitCrabInput>
  <Language>
    <Name>DuplicateSlotNameProbe</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cX"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cY"><Representations><Representation>y</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered">
        <Name>Main</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrP1">
            <MorphemeId>P1</MorphemeId>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="mrP1s1">
                <MorphologicalInput>
                  <PhoneticSequence><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput>
                  <InsertSegments><PhoneticShape>x</PhoneticShape></InsertSegments>
                  <CopyFromInput />
                </MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
          <MorphologicalRule id="mrP2">
            <MorphemeId>P2</MorphemeId>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="mrP2s1">
                <MorphologicalInput>
                  <PhoneticSequence><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput>
                  <InsertSegments><PhoneticShape>y</PhoneticShape></InsertSegments>
                  <CopyFromInput />
                </MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <AffixTemplates>
          <AffixTemplate requiredPartsOfSpeech="posV">
            <Name>TemplateA</Name>
            <Slot optional="true" morphologicalRules="mrP1"><Name>Root</Name></Slot>
          </AffixTemplate>
          <AffixTemplate requiredPartsOfSpeech="posV">
            <Name>TemplateB</Name>
            <Slot optional="true" morphologicalRules="mrP2"><Name>Root</Name></Slot>
          </AffixTemplate>
        </AffixTemplates>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
'@
	$duplicateSlotNameGrammarPath = Join-Path $parserRefusalTempRoot 'duplicate-slot-name.grammar.xml'
	Set-Content -Path $duplicateSlotNameGrammarPath -Value $duplicateSlotNameGrammarXml -Encoding utf8
	$duplicateSlotNameOutDir = Join-Path $parserRefusalTempRoot 'duplicate-slot-name-out'
	Assert-ProjectorExit -ExePath $exePath -Label 'Duplicate slot-name refusal probe' -ExpectedExitCode 7 -ExpectedSubstrings 'Root' -Arguments @(
		'author', '--grammar', $duplicateSlotNameGrammarPath, '--out-dir', $duplicateSlotNameOutDir, '--name', 'DuplicateSlotName'
	) | Out-Null
	$duplicateSlotNameFwdata = Join-Path $duplicateSlotNameOutDir 'DuplicateSlotName\DuplicateSlotName.fwdata'
	if (Test-Path $duplicateSlotNameFwdata) {
		Write-Error "Duplicate slot-name refusal probe: exit was 7 but a .fwdata was left on disk at $duplicateSlotNameFwdata."
		exit 1
	}
	Write-Host "Duplicate slot-name refusal probe OK: exit 7, output names 'Root', no .fwdata on disk."
}
finally {
	Remove-Item -Path $parserRefusalTempRoot -Recurse -Force -ErrorAction SilentlyContinue
}

# --- author / verify-parity live tests: the `machine` conformance submodule's own fixtures,
#     not Sena 3 -- independent of whether Sena3 is reachable above. ---
$conformanceRoot = Join-Path $root '..\..\machine\conformance'
$pilotGrammar = Join-Path $conformanceRoot 'edge-cases\deep-optional-affix-nesting\grammar.xml'
$mprRefusalGrammar = Join-Path $conformanceRoot 'languages\prefixal-discontinuous-slot-dependency\grammar.xml'
$requireRefusalGrammar = Join-Path $conformanceRoot 'languages\suffixing-evidential-adjacency-chain\grammar.xml'

# --- AllomorphCoOccurrenceRule authoring probe: runs above the machine-submodule gate below
#     (this fixture is this tool's own testdata, not part of the machine submodule, so it needs no
#     submodule at all) and pins the fix for the silent-drop defect (a type="exclude"
#     AllomorphCoOccurrenceRule used to author with no IMoAlloAdhocProhib created and no refusal at
#     all -- see GrammarAuthor.CreateCoOccurrenceRules); it also proves the exclusion actually
#     binds in the live HC engine, not just that an object was created. ---
$alloCoOccurGrammar = Join-Path $root 'testdata\allomorph-cooccurrence-probe.grammar.xml'
$alloCoOccurTempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("xample-projector-allo-cooccur-" + [System.Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $alloCoOccurTempRoot -Force | Out-Null
try {
	$alloCoOccurOutDir = Join-Path $alloCoOccurTempRoot 'allo-cooccur-out'
	New-Item -ItemType Directory -Path $alloCoOccurOutDir -Force | Out-Null
	& $exePath author --grammar $alloCoOccurGrammar --out-dir $alloCoOccurOutDir --name AlloCoOccur
	if ($LASTEXITCODE -ne 0) {
		Write-Error "AllomorphCoOccurrenceRule authoring probe: 'author' failed (exit $LASTEXITCODE)."
		exit 1
	}
	$alloCoOccurResponsePath = Join-Path $alloCoOccurOutDir 'author-response.json'
	$alloCoOccurResponse = Get-Content $alloCoOccurResponsePath -Raw | ConvertFrom-Json
	if ($alloCoOccurResponse.authored.MoAlloAdhocProhib -ne 2) {
		Write-Error "AllomorphCoOccurrenceRule authoring probe: expected authored.MoAlloAdhocProhib == 2, got '$($alloCoOccurResponse.authored.MoAlloAdhocProhib)'."
		exit 1
	}
	foreach ($guidMapKey in @('allomorphCoOccurrence[0]', 'allomorphCoOccurrence[1]')) {
		if (-not ($alloCoOccurResponse.guidMap.PSObject.Properties.Name -contains $guidMapKey)) {
			Write-Error "AllomorphCoOccurrenceRule authoring probe: guidMap is missing '$guidMapKey'."
			exit 1
		}
	}
	Write-Host "AllomorphCoOccurrenceRule authoring probe OK: exit 0, authored.MoAlloAdhocProhib == 2, guidMap has both 'allomorphCoOccurrence[0]' and 'allomorphCoOccurrence[1]'."

	$alloCoOccurFwdata = Join-Path $alloCoOccurOutDir 'AlloCoOccur\AlloCoOccur.fwdata'
	$alloCoOccurProjectedOutDir = Join-Path $alloCoOccurTempRoot 'projected'
	New-Item -ItemType Directory -Path $alloCoOccurProjectedOutDir -Force | Out-Null
	& $exePath project --project $alloCoOccurFwdata --out-dir $alloCoOccurProjectedOutDir --database AlloCoOccur
	if ($LASTEXITCODE -ne 0) {
		Write-Error "AllomorphCoOccurrenceRule authoring probe: 'project' (on the authored project) failed (exit $LASTEXITCODE)."
		exit 1
	}
	$alloCoOccurHcXml = Join-Path $alloCoOccurProjectedOutDir 'AlloCoOccur.hc.xml'

	# The probe grammar has one optional slot (mrP1/mrP2, prefixes "x"/"y") over two lexical entries
	# ("k"/"l"), and excludes each rule's own subrule from co-occurring with ITS OWN paired entry's
	# allomorph "anywhere" in a word -- the ONLY way to produce "xk" (resp. "yl") at all is that
	# exact combination, so a correct exclusion makes "xk" and "yl" unparseable (0 analyses) while
	# the affixless "k"/"l" still parse (1 analysis each). This is the probe's own designed
	# semantics, not an oracle-confirmed count (this fixture is not a `machine` conformance
	# fixture) -- unlike the pilot fixture's counts below.
	$alloCoOccurVerifyOutput = & $exePath verify-parity --grammar $alloCoOccurGrammar --hc-xml $alloCoOccurHcXml --guid-map $alloCoOccurResponsePath --expect k=1 --expect xk=0 --expect l=1 --expect yl=0
	if ($LASTEXITCODE -ne 0) {
		Write-Error "AllomorphCoOccurrenceRule authoring probe: 'verify-parity' failed (exit $LASTEXITCODE). Output:`n$($alloCoOccurVerifyOutput | Out-String)"
		exit 1
	}
	$alloCoOccurVerifyJson = $alloCoOccurVerifyOutput | Out-String | ConvertFrom-Json
	if ($alloCoOccurVerifyJson.engineAnalysisCounts.k -ne 1 -or $alloCoOccurVerifyJson.engineAnalysisCounts.xk -ne 0 -or
		$alloCoOccurVerifyJson.engineAnalysisCounts.l -ne 1 -or $alloCoOccurVerifyJson.engineAnalysisCounts.yl -ne 0) {
		Write-Error "AllomorphCoOccurrenceRule authoring probe: verify-parity reported unexpected engine counts:`n$($alloCoOccurVerifyOutput | Out-String)"
		exit 1
	}
	Write-Host "AllomorphCoOccurrenceRule authoring probe OK: verify-parity confirms both exclusions bind in the live HC engine (k=1, xk=0, l=1, yl=0)."

	Test-ContentDerivedLabelCatchesCoOccurrenceSwap -SourceFwdata $alloCoOccurFwdata -ExePath $exePath -Label 'AllomorphCoOccurrenceRule authoring probe'
}
finally {
	Remove-Item -Path $alloCoOccurTempRoot -Recurse -Force -ErrorAction SilentlyContinue
}

if (-not (Test-Path $pilotGrammar)) {
	Write-Host "SKIPPED (author/verify-parity live tests): machine submodule fixture not found at $pilotGrammar"
	exit 0
}

$authorTempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("xample-projector-author-test-" + [System.Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $authorTempRoot -Force | Out-Null
try {
	# --- author the pilot fixture, project it, and verify-parity against the fixture it came from ---
	$authorOutDir = Join-Path $authorTempRoot 'author-out'
	New-Item -ItemType Directory -Path $authorOutDir -Force | Out-Null
	Write-Host "Running: $exePath author --grammar `"$pilotGrammar`" --out-dir `"$authorOutDir`" --name Pilot"
	& $exePath author --grammar $pilotGrammar --out-dir $authorOutDir --name Pilot
	if ($LASTEXITCODE -ne 0) {
		Write-Error "Live 'author' run failed (exit $LASTEXITCODE)."
		exit $LASTEXITCODE
	}
	$authorResponsePath = Join-Path $authorOutDir 'author-response.json'
	& $exePath --validate-capture $authorResponsePath
	if ($LASTEXITCODE -ne 0) {
		Write-Error "--validate-capture failed on the live 'author' response.json (exit $LASTEXITCODE)."
		exit $LASTEXITCODE
	}
	Write-Host 'Live author run OK.'

	$pilotFwdata = Join-Path $authorOutDir 'Pilot\Pilot.fwdata'
	$projectedOutDir = Join-Path $authorTempRoot 'projected'
	New-Item -ItemType Directory -Path $projectedOutDir -Force | Out-Null
	Write-Host "Running: $exePath project --project `"$pilotFwdata`" --out-dir `"$projectedOutDir`" --database Pilot"
	& $exePath project --project $pilotFwdata --out-dir $projectedOutDir --database Pilot
	if ($LASTEXITCODE -ne 0) {
		Write-Error "Live 'project' run (on the authored pilot project) failed (exit $LASTEXITCODE)."
		exit $LASTEXITCODE
	}
	Write-Host 'Live project run (on the authored pilot project) OK.'

	$pilotHcXml = Join-Path $projectedOutDir 'Pilot.hc.xml'
	# The pilot's own oracle-confirmed words.yaml is where 1/924 come from (see README) -- the only
	# two counts this build.ps1 asserts are ones a real oracle run backs, per this repo's oracle
	# discipline; verify-parity itself derives every other expectation from grammar.xml.
	Write-Host "Running: $exePath verify-parity --grammar `"$pilotGrammar`" --hc-xml `"$pilotHcXml`" --guid-map `"$authorResponsePath`" --expect k=1 --expect xxxxxxk=924"
	$verifyOutput = & $exePath verify-parity --grammar $pilotGrammar --hc-xml $pilotHcXml --guid-map $authorResponsePath --expect k=1 --expect xxxxxxk=924
	if ($LASTEXITCODE -ne 0) {
		Write-Error "'verify-parity' failed (exit $LASTEXITCODE). Output:`n$($verifyOutput | Out-String)"
		exit $LASTEXITCODE
	}
	$verifyJson = $verifyOutput | Out-String | ConvertFrom-Json
	if ($verifyJson.morphologicalRuleCount -ne 12 -or $verifyJson.lexicalEntryCount -ne 1 -or $verifyJson.segmentCount -ne 2 -or
		$verifyJson.slotCount -ne 12 -or $verifyJson.xampleLexEntryCount -ne 13 -or
		$verifyJson.engineAnalysisCounts.k -ne 1 -or $verifyJson.engineAnalysisCounts.xxxxxxk -ne 924) {
		Write-Error "verify-parity reported unexpected counts:`n$($verifyOutput | Out-String)"
		exit 1
	}
	Write-Host "verify-parity OK: 12 rules, 1 lex entry, 2 segments, 12 slots (order: $($verifyJson.slotOrderingRule)), 13 XAMPLE lex entries, HC engine 1/xxxxxxk=924 analyses."
	if ($verifyJson.guidMapVerifiedCount -le 0) {
		Write-Error "verify-parity reported guidMapVerifiedCount <= 0 -- the guid-map binding check did not run."
		exit 1
	}
	Write-Host "verify-parity guid-map binding OK: $($verifyJson.guidMapVerifiedCount) guidMap entries checked against the live authored project."

	# --- verify-parity guid-map binding probes: a swapped or emptied guid map must be refused
	#     (exit 8), not silently accepted -- both copies live beside the real author-response.json
	#     so its "projectPath" (relative to that directory) still resolves. ---
	$swappedResponse = Get-Content $authorResponsePath -Raw | ConvertFrom-Json
	$slot1Guid = $swappedResponse.guidMap.slot1
	$slot2Guid = $swappedResponse.guidMap.slot2
	$swappedResponse.guidMap.slot1 = $slot2Guid
	$swappedResponse.guidMap.slot2 = $slot1Guid
	$swappedResponsePath = Join-Path $authorOutDir 'author-response-swapped-slots.json'
	$swappedResponse | ConvertTo-Json -Depth 10 | Set-Content -Path $swappedResponsePath -Encoding utf8
	Assert-ProjectorExit -ExePath $exePath -Label 'verify-parity guid-map binding probe (swapped slots)' -ExpectedExitCode 8 -ExpectedSubstrings 'order' -Arguments @(
		'verify-parity', '--grammar', $pilotGrammar, '--hc-xml', $pilotHcXml, '--guid-map', $swappedResponsePath, '--expect', 'k=1', '--expect', 'xxxxxxk=924'
	) | Out-Null
	Write-Host "verify-parity guid-map binding probe (swapped slot1/slot2 guids) OK: exit 8, output names the order difference."

	$emptyResponse = Get-Content $authorResponsePath -Raw | ConvertFrom-Json
	$emptyResponse.guidMap = New-Object PSObject
	$emptyResponsePath = Join-Path $authorOutDir 'author-response-empty-guidmap.json'
	$emptyResponse | ConvertTo-Json -Depth 10 | Set-Content -Path $emptyResponsePath -Encoding utf8
	Assert-ProjectorExit -ExePath $exePath -Label 'verify-parity guid-map binding probe (empty guidMap)' -ExpectedExitCode 8 -ExpectedSubstrings 'missing fixture id' -Arguments @(
		'verify-parity', '--grammar', $pilotGrammar, '--hc-xml', $pilotHcXml, '--guid-map', $emptyResponsePath, '--expect', 'k=1', '--expect', 'xxxxxxk=924'
	) | Out-Null
	Write-Host "verify-parity guid-map binding probe (emptied guidMap) OK: exit 8, output names the missing key(s)."

	# --- determinism: author the SAME fixture into a second directory; structure (authored counts,
	#     guidMap key set) must match exactly even though GUIDs/timestamps make the .fwdata bytes differ ---
	$authorOutDir2 = Join-Path $authorTempRoot 'author-out-2'
	New-Item -ItemType Directory -Path $authorOutDir2 -Force | Out-Null
	& $exePath author --grammar $pilotGrammar --out-dir $authorOutDir2 --name Pilot | Out-Null
	if ($LASTEXITCODE -ne 0) {
		Write-Error "Second live 'author' run (determinism probe) failed (exit $LASTEXITCODE)."
		exit $LASTEXITCODE
	}
	$authorResponse1 = Get-Content $authorResponsePath -Raw | ConvertFrom-Json
	$authorResponse2 = Get-Content (Join-Path $authorOutDir2 'author-response.json') -Raw | ConvertFrom-Json
	$keys1 = ($authorResponse1.guidMap.PSObject.Properties.Name | Sort-Object) -join ','
	$keys2 = ($authorResponse2.guidMap.PSObject.Properties.Name | Sort-Object) -join ','
	if ($keys1 -ne $keys2) {
		Write-Error "Determinism probe: guidMap key sets differ between two 'author' runs over the same fixture."
		exit 1
	}
	$counts1 = ($authorResponse1.authored.PSObject.Properties | ForEach-Object { "$($_.Name)=$($_.Value)" } | Sort-Object) -join ','
	$counts2 = ($authorResponse2.authored.PSObject.Properties | ForEach-Object { "$($_.Name)=$($_.Value)" } | Sort-Object) -join ','
	if ($counts1 -ne $counts2) {
		Write-Error "Determinism probe: authored counts differ between two 'author' runs over the same fixture."
		exit 1
	}
	if ($authorResponse1.projectSha256 -eq $authorResponse2.projectSha256) {
		Write-Host "Determinism probe note: projectSha256 happened to match across two runs (LibLCM guids are random, so this is not required to differ, only permitted to)."
	}
	Write-Host "Determinism probe OK: $($authorResponse1.guidMap.PSObject.Properties.Name.Count) guidMap keys and every authored count match across two independent 'author' runs (sha256 legitimately differs by run, since LCM guids are assigned randomly)."

	# --- determinism, strengthened: normalize both .fwdata files (known guids -> fixture id,
	#     every remaining guid/timestamp -> a fixed placeholder) and diff them directly, rather
	#     than trusting guidMap key sets and authored counts as a proxy for the project itself ---
	$pilotFwdata2 = Join-Path $authorOutDir2 'Pilot\Pilot.fwdata'
	$normalizedFwdata1 = Get-NormalizedFwdataText -Path $pilotFwdata -GuidToFixtureId (Get-GuidToFixtureIdMap $authorResponse1)
	$normalizedFwdata2 = Get-NormalizedFwdataText -Path $pilotFwdata2 -GuidToFixtureId (Get-GuidToFixtureIdMap $authorResponse2)
	if ($normalizedFwdata1 -ne $normalizedFwdata2) {
		$diffLines = Compare-Object -ReferenceObject ($normalizedFwdata1 -split "`r?`n") -DifferenceObject ($normalizedFwdata2 -split "`r?`n")
		Write-Error "Normalized .fwdata determinism probe: normalized files differ ($($diffLines.Count) differing line(s)). First 10:`n$(($diffLines | Select-Object -First 10 | Out-String))"
		exit 1
	}
	Write-Host "Normalized .fwdata determinism probe OK: two independent 'author' runs produce byte-identical .fwdata text after mapping guidMap guids to fixture ids and blanking every remaining guid/timestamp ($($normalizedFwdata1.Length) chars)."

	# --- refusal probes: exit 7, naming the unsupported construct ---
	if (Test-Path $mprRefusalGrammar) {
		$mprOutDir = Join-Path $authorTempRoot 'mpr-refusal-out'
		New-Item -ItemType Directory -Path $mprOutDir -Force | Out-Null
		Assert-ProjectorExit -ExePath $exePath -Label 'MPRFeatures refusal probe' -ExpectedExitCode 7 -ExpectedSubstrings 'mrModeTrans', 'MPRFeatures' -Arguments @(
			'author', '--grammar', $mprRefusalGrammar, '--out-dir', $mprOutDir, '--name', 'MprRefusal'
		) | Out-Null
		Write-Host "MPRFeatures refusal probe OK: exit 7, output names mrModeTrans's MPRFeatures."
	}
	else {
		Write-Host "SKIPPED (MPRFeatures refusal probe): fixture not found at $mprRefusalGrammar"
	}

	if (Test-Path $requireRefusalGrammar) {
		$requireOutDir = Join-Path $authorTempRoot 'require-refusal-out'
		New-Item -ItemType Directory -Path $requireOutDir -Force | Out-Null
		Assert-ProjectorExit -ExePath $exePath -Label 'type="require" refusal probe' -ExpectedExitCode 7 -ExpectedSubstrings 'type="require"' -Arguments @(
			'author', '--grammar', $requireRefusalGrammar, '--out-dir', $requireOutDir, '--name', 'RequireRefusal'
		) | Out-Null
		Write-Host "type=`"require`" refusal probe OK: exit 7, output names type=`"require`"."
	}
	else {
		Write-Host "SKIPPED (type=`"require`" refusal probe): fixture not found at $requireRefusalGrammar"
	}
}
finally {
	Remove-Item -Path $authorTempRoot -Recurse -Force -ErrorAction SilentlyContinue
}

# --- mutate/parse live proof: 'mutate' + 'parse' against a freshly authored pilot project.
#     Skipped (with reason) only when FieldWorks itself is absent -- already checked at the top of
#     this script, so reaching here means FieldWorks is present. The Machine grammar root defaults
#     to a fixed path and is independently overridable via PANGLOSS_MACHINE_DIR, distinct from this
#     repo's own `machine` submodule ($conformanceRoot above); the checked-in FieldWorks witness
#     below swaps in a real project for this same fixture when present. ---
$machineRootForMutateParse = $env:PANGLOSS_MACHINE_DIR
if ([string]::IsNullOrEmpty($machineRootForMutateParse)) { $machineRootForMutateParse = 'C:\Users\johnm\Documents\repos\machine' }
$mutateParseConformanceDir = Join-Path $machineRootForMutateParse 'conformance'
$mutateParseGrammar = Join-Path $mutateParseConformanceDir 'edge-cases\deep-optional-affix-nesting\grammar.xml'

if (-not (Test-Path $mutateParseGrammar)) {
	Write-Host "SKIPPED (mutate/parse live proof): grammar not found at $mutateParseGrammar"
	exit 0
}

# Prefer the checked-in FieldWorks witness (machine\conformance\PROTOCOL.md section 10) over
# authoring a fresh base project -- it IS a real, oracle-adjacent project rather than one this run
# just invented, and using it here is what keeps it from silently drifting away from what 'author'
# actually produces. Absence is not an error (an older or partial machine checkout) -- name the
# path and fall back, as today.
$witnessDir = Join-Path $mutateParseConformanceDir 'edge-cases\deep-optional-affix-nesting\fieldworks'
$witnessFwdata = Join-Path $witnessDir 'project.fwdata'
$witnessManifestPath = Join-Path $witnessDir 'phonology-mutations.yaml'
$witnessFwdataPresent = Test-Path $witnessFwdata
$witnessManifestPresent = Test-Path $witnessManifestPath
if ($witnessFwdataPresent -ne $witnessManifestPresent) {
	Write-Error "mutate/parse live proof: fieldworks witness is partially present under $witnessDir (project.fwdata=$witnessFwdataPresent, phonology-mutations.yaml=$witnessManifestPresent) -- expected both or neither."
	exit 1
}
$usingWitness = $witnessFwdataPresent -and $witnessManifestPresent
if (-not $usingWitness) {
	Write-Host "SKIPPED (checked-in FieldWorks witness): not found at $witnessFwdata -- falling back to a freshly authored base project for this run."
}

$mpTempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("xample-projector-mutate-parse-" + [System.Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $mpTempRoot -Force | Out-Null
try {
	# --- 1. Acquire the base project for this part: the checked-in witness when present, else
	#        author the pilot fixture fresh (unchanged fallback behavior). ---
	if ($usingWitness) {
		$manifest = ConvertFrom-PhonologyMutationsYaml -Path $witnessManifestPath

		$witnessActualSha256 = (Get-FileHash -Algorithm SHA256 -Path $witnessFwdata).Hash.ToLowerInvariant()
		if ($witnessActualSha256 -ne $manifest.baseSha256) {
			Write-Error "mutate/parse live proof: checked-in witness sha256 mismatch -- $witnessFwdata hashes to $witnessActualSha256, but phonology-mutations.yaml declares base_sha256 $($manifest.baseSha256)."
			exit 1
		}
		Write-Host "mutate/parse live proof: using the checked-in FieldWorks witness at $witnessFwdata (sha256 $witnessActualSha256, verified against phonology-mutations.yaml)."

		# Never touch the machine checkout's own copy: opening a project, even read-only, can leave
		# session artifacts beside it (this same witness's own first 'inspect' left a SharedSettings
		# directory that had to be deleted before it was committed) -- copy it out first.
		$witnessCopyDir = Join-Path $mpTempRoot 'witness-base\project'
		New-Item -ItemType Directory -Path $witnessCopyDir -Force | Out-Null
		Copy-Item -Path $witnessFwdata -Destination (Join-Path $witnessCopyDir 'project.fwdata') -Force
		New-Item -ItemType Directory -Path (Join-Path $witnessCopyDir 'WritingSystemStore') -Force | Out-Null
		Get-ChildItem (Join-Path $witnessDir 'WritingSystemStore') -Filter '*.ldml' | ForEach-Object {
			Copy-Item -Path $_.FullName -Destination (Join-Path $witnessCopyDir "WritingSystemStore\$($_.Name)") -Force
		}
		$baseFwdata = Join-Path $witnessCopyDir 'project.fwdata'
		$baseSha256Before = (Get-FileHash -Algorithm SHA256 -Path $baseFwdata).Hash.ToLowerInvariant()
		if ($baseSha256Before -ne $manifest.baseSha256) {
			Write-Error "mutate/parse live proof: the working copy of the witness ($baseSha256Before) does not match the verified original ($($manifest.baseSha256)) -- copy corrupted?"
			exit 1
		}

		# The witness must never silently drift from what 'author' actually produces: author the SAME
		# grammar.xml fresh and compare -- both sides labeled independently by their OWN content
		# (neither a shared map nor a blanket blind), since the witness has no committed guidMap.
		$driftAuthorOut = Join-Path $mpTempRoot 'witness-drift-author'
		New-Item -ItemType Directory -Path $driftAuthorOut -Force | Out-Null
		& $exePath author --grammar $mutateParseGrammar --out-dir $driftAuthorOut --name WitnessDrift
		if ($LASTEXITCODE -ne 0) { Write-Error "mutate/parse live proof: witness-drift 'author' failed (exit $LASTEXITCODE)."; exit $LASTEXITCODE }
		$driftFwdata = Join-Path $driftAuthorOut 'WitnessDrift\WitnessDrift.fwdata'
		$driftNormalizedFresh = Get-NormalizedFwdataText -Path $driftFwdata -GuidToFixtureId (Get-ContentDerivedGuidLabelMap -Path $driftFwdata -CheckExePath $exePath)
		$driftNormalizedWitness = Get-NormalizedFwdataText -Path $baseFwdata -GuidToFixtureId (Get-ContentDerivedGuidLabelMap -Path $baseFwdata -CheckExePath $exePath)
		if ($driftNormalizedFresh -ne $driftNormalizedWitness) {
			$driftDiffLines = Compare-Object -ReferenceObject ($driftNormalizedFresh -split "`r?`n") -DifferenceObject ($driftNormalizedWitness -split "`r?`n")
			Write-Error "mutate/parse live proof: the checked-in witness has drifted from 'author' ($($driftDiffLines.Count) differing normalized line(s)). First 10:`n$(($driftDiffLines | Select-Object -First 10 | Out-String))"
			exit 1
		}
		Write-Host "mutate/parse live proof: witness-drift probe OK -- a freshly authored pilot normalizes identically to the checked-in witness ($($driftNormalizedFresh.Length) chars)."
	}
	else {
		$baseAuthorOut = Join-Path $mpTempRoot 'base-author'
		New-Item -ItemType Directory -Path $baseAuthorOut -Force | Out-Null
		& $exePath author --grammar $mutateParseGrammar --out-dir $baseAuthorOut --name MutateParseBase
		if ($LASTEXITCODE -ne 0) { Write-Error "mutate/parse live proof: base 'author' failed (exit $LASTEXITCODE)."; exit $LASTEXITCODE }
		$baseFwdata = Join-Path $baseAuthorOut 'MutateParseBase\MutateParseBase.fwdata'
		$baseSha256Before = (Get-FileHash -Algorithm SHA256 -Path $baseFwdata).Hash.ToLowerInvariant()
		Write-Host "mutate/parse live proof: base project authored, sha256 $baseSha256Before"
	}

	Test-ContentDerivedLabelCatchesWiringSwap -SourceFwdata $baseFwdata -ExePath $exePath -Label 'mutate/parse live proof base project'

	# --- 'mutate' schemaVersion refusal probe: a request declaring an unsupported schemaVersion
	#     must be refused (exit 9), naming the version found and the version supported, rather than
	#     silently treated as schemaVersion 1. ---
	$badSchemaVersionRequestObj = @{
		schemaVersion = 2
		caseId        = 'bad-schema-version'
		baseSha256    = $baseSha256Before
		operations    = @(@{ op = 'remove_all_phonemes'; requireUnreferenced = $true })
	}
	$badSchemaVersionRequestPath = Join-Path $mpTempRoot 'bad-schema-version-request.json'
	($badSchemaVersionRequestObj | ConvertTo-Json -Depth 5) | Set-Content -Path $badSchemaVersionRequestPath -Encoding utf8
	$badSchemaVersionOutDir = Join-Path $mpTempRoot 'bad-schema-version-out'
	Assert-ProjectorExit -ExePath $exePath -Label 'mutate schemaVersion refusal probe' -ExpectedExitCode 9 -ExpectedSubstrings '2', '1' -Arguments @(
		'mutate', '--project', $baseFwdata, '--request', $badSchemaVersionRequestPath, '--out-dir', $badSchemaVersionOutDir
	) | Out-Null
	$baseSha256AfterBadSchemaVersion = (Get-FileHash -Algorithm SHA256 -Path $baseFwdata).Hash.ToLowerInvariant()
	if ($baseSha256AfterBadSchemaVersion -ne $baseSha256Before) {
		Write-Error "mutate schemaVersion refusal probe: SOURCE PROJECT WAS MODIFIED (sha256 $baseSha256Before -> $baseSha256AfterBadSchemaVersion)"
		exit 1
	}
	Write-Host "mutate schemaVersion refusal probe OK: exit 9, output names schemaVersion 2 and 1, source sha256 unchanged."

	# --- Record the "k" phoneme guid via inspect. ---
	$baseInspectPath = Join-Path $mpTempRoot 'base-inspect.json'
	& $exePath inspect --project $baseFwdata --out $baseInspectPath
	if ($LASTEXITCODE -ne 0) { Write-Error "mutate/parse live proof: base 'inspect' failed (exit $LASTEXITCODE)."; exit $LASTEXITCODE }
	$baseInspect = Get-Content $baseInspectPath -Raw | ConvertFrom-Json
	$kPhoneme = $baseInspect.phonemes | Where-Object { $_.representations -contains 'k' } | Select-Object -First 1
	if (-not $kPhoneme) { Write-Error "mutate/parse live proof: no phoneme with representation 'k' found in $baseInspectPath"; exit 1 }
	$kGuid = $kPhoneme.guid
	Write-Host "mutate/parse live proof: 'k' phoneme guid = $kGuid"

	# --- project the base project once: gives the byte-identity/hc.xml baseline AND parse's own adctl/gram/lex. ---
	$baseProjectedOut = Join-Path $mpTempRoot 'base-projected'
	New-Item -ItemType Directory -Path $baseProjectedOut -Force | Out-Null
	& $exePath project --project $baseFwdata --out-dir $baseProjectedOut --database MPBase
	if ($LASTEXITCODE -ne 0) { Write-Error "mutate/parse live proof: base 'project' failed (exit $LASTEXITCODE)."; exit $LASTEXITCODE }

	# --- Case A: remove-k-only ---
	if ($usingWitness) {
		$removeKCase = $manifest.cases | Where-Object { $_.id -eq 'remove-k-only' }
		if (-not $removeKCase) { Write-Error "mutate/parse live proof: witness manifest has no 'remove-k-only' case."; exit 1 }
		$removeKRequestObj = Get-MutationRequestFromManifestCase -Case $removeKCase -BaseSha256 $baseSha256Before
	}
	else {
		$removeKRequestObj = @{
			schemaVersion = 1
			caseId        = 'remove-k-only'
			baseSha256    = $baseSha256Before
			operations    = @(@{ op = 'remove_phoneme'; guid = $kGuid; assertRepresentations = @('k'); requireUnreferenced = $true })
		}
	}
	$removeKRequestPath = Join-Path $mpTempRoot 'remove-k-only-request.json'
	($removeKRequestObj | ConvertTo-Json -Depth 5) | Set-Content -Path $removeKRequestPath -Encoding utf8
	$removeKOutDir = Join-Path $mpTempRoot 'remove-k-only-out'
	& $exePath mutate --project $baseFwdata --request $removeKRequestPath --out-dir $removeKOutDir
	if ($LASTEXITCODE -ne 0) { Write-Error "mutate/parse live proof: 'remove-k-only' mutate failed (exit $LASTEXITCODE)."; exit $LASTEXITCODE }
	$removeKResponsePath = Join-Path $removeKOutDir 'mutation-response.json'
	& $exePath --validate-capture $removeKResponsePath
	if ($LASTEXITCODE -ne 0) { Write-Error "mutate/parse live proof: --validate-capture failed on remove-k-only's mutation-response.json (exit $LASTEXITCODE)."; exit $LASTEXITCODE }
	$removeKResponse = Get-Content $removeKResponsePath -Raw | ConvertFrom-Json
	if ($removeKResponse.deletedCount -ne 1) { Write-Error "remove-k-only: expected deletedCount 1, got $($removeKResponse.deletedCount)"; exit 1 }
	if ($removeKResponse.removed.Count -ne 1 -or $removeKResponse.removed[0].representations.Count -ne 1 -or $removeKResponse.removed[0].representations[0] -ne 'k') {
		Write-Error "remove-k-only: unexpected removed[]:`n$($removeKResponse.removed | ConvertTo-Json -Depth 5)"
		exit 1
	}
	if (-not $removeKResponse.reopened) { Write-Error "remove-k-only: reopened was not true"; exit 1 }
	$baseSha256AfterA = (Get-FileHash -Algorithm SHA256 -Path $baseFwdata).Hash.ToLowerInvariant()
	if ($baseSha256AfterA -ne $baseSha256Before) { Write-Error "remove-k-only: SOURCE PROJECT WAS MODIFIED (sha256 $baseSha256Before -> $baseSha256AfterA)"; exit 1 }
	Write-Host "mutate 'remove-k-only' OK: deletedCount=1, removed=['k'], reopened=true, source sha256 unchanged ($baseSha256Before)."
	if ($usingWitness) {
		$actualInferredSegsA = ($removeKResponse.removed | ForEach-Object { $_.representations[0] } | Sort-Object) -join ','
		$expectedInferredSegsA = ($removeKCase.expectInferredSegments | Sort-Object) -join ','
		if ($actualInferredSegsA -ne $expectedInferredSegsA) {
			Write-Error "remove-k-only: manifest expect.inferred_segments [$expectedInferredSegsA] does not match actual removed representations [$actualInferredSegsA]"
			exit 1
		}
		Write-Host "remove-k-only: manifest expect.inferred_segments confirmed ($expectedInferredSegsA)."
	}

	$removeKClonedFwdata = Join-Path $removeKOutDir $removeKResponse.materializedProjectPath
	$removeKProjectedOut = Join-Path $mpTempRoot 'remove-k-only-projected'
	New-Item -ItemType Directory -Path $removeKProjectedOut -Force | Out-Null
	& $exePath project --project $removeKClonedFwdata --out-dir $removeKProjectedOut --database MPBase
	if ($LASTEXITCODE -ne 0) { Write-Error "mutate/parse live proof: 'project' on the remove-k-only clone failed (exit $LASTEXITCODE)."; exit $LASTEXITCODE }
	Test-XampleFilesEquivalentIgnoringHvoRenumbering -BaseDir $baseProjectedOut -CloneDir $removeKProjectedOut -Database 'MPBase' -Label 'remove-k-only'
	Test-XampleFileCorruptionIsCaught -CloneDir $removeKProjectedOut -Database 'MPBase' -Label 'remove-k-only'
	Assert-HcXmlLacksSegmentDefinition -BaseHcXmlPath (Join-Path $baseProjectedOut 'MPBase.hc.xml') -CloneHcXmlPath (Join-Path $removeKProjectedOut 'MPBase.hc.xml') -Representation 'k' -Label 'remove-k-only'

	# --- Case B: empty-phoneme-inventory ---
	if ($usingWitness) {
		$emptyCase = $manifest.cases | Where-Object { $_.id -eq 'empty-phoneme-inventory' }
		if (-not $emptyCase) { Write-Error "mutate/parse live proof: witness manifest has no 'empty-phoneme-inventory' case."; exit 1 }
		$emptyRequestObj = Get-MutationRequestFromManifestCase -Case $emptyCase -BaseSha256 $baseSha256Before
	}
	else {
		$emptyRequestObj = @{
			schemaVersion = 1
			caseId        = 'empty-phoneme-inventory'
			baseSha256    = $baseSha256Before
			operations    = @(@{ op = 'remove_all_phonemes'; requireUnreferenced = $true })
		}
	}
	$emptyRequestPath = Join-Path $mpTempRoot 'empty-phoneme-inventory-request.json'
	($emptyRequestObj | ConvertTo-Json -Depth 5) | Set-Content -Path $emptyRequestPath -Encoding utf8
	$emptyOutDir = Join-Path $mpTempRoot 'empty-phoneme-inventory-out'
	& $exePath mutate --project $baseFwdata --request $emptyRequestPath --out-dir $emptyOutDir
	if ($LASTEXITCODE -ne 0) { Write-Error "mutate/parse live proof: 'empty-phoneme-inventory' mutate failed (exit $LASTEXITCODE)."; exit $LASTEXITCODE }
	$emptyResponsePath = Join-Path $emptyOutDir 'mutation-response.json'
	& $exePath --validate-capture $emptyResponsePath
	if ($LASTEXITCODE -ne 0) { Write-Error "mutate/parse live proof: --validate-capture failed on empty-phoneme-inventory's mutation-response.json (exit $LASTEXITCODE)."; exit $LASTEXITCODE }
	$emptyResponse = Get-Content $emptyResponsePath -Raw | ConvertFrom-Json
	if ($emptyResponse.deletedCount -ne 2) { Write-Error "empty-phoneme-inventory: expected deletedCount 2, got $($emptyResponse.deletedCount)"; exit 1 }
	$emptyReps = ($emptyResponse.removed | ForEach-Object { $_.representations[0] } | Sort-Object) -join ','
	if ($emptyReps -ne 'k,x') { Write-Error "empty-phoneme-inventory: expected removed representations k,x -- got $emptyReps"; exit 1 }
	if (-not $emptyResponse.reopened) { Write-Error "empty-phoneme-inventory: reopened was not true"; exit 1 }
	$baseSha256AfterB = (Get-FileHash -Algorithm SHA256 -Path $baseFwdata).Hash.ToLowerInvariant()
	if ($baseSha256AfterB -ne $baseSha256Before) { Write-Error "empty-phoneme-inventory: SOURCE PROJECT WAS MODIFIED (sha256 $baseSha256Before -> $baseSha256AfterB)"; exit 1 }
	Write-Host "mutate 'empty-phoneme-inventory' OK: deletedCount=2, removed=[k,x], reopened=true, source sha256 unchanged ($baseSha256Before)."
	if ($usingWitness) {
		$expectedInferredSegsB = ($emptyCase.expectInferredSegments | Sort-Object) -join ','
		if ($emptyReps -ne $expectedInferredSegsB) {
			Write-Error "empty-phoneme-inventory: manifest expect.inferred_segments [$expectedInferredSegsB] does not match actual removed representations [$emptyReps]"
			exit 1
		}
		Write-Host "empty-phoneme-inventory: manifest expect.inferred_segments confirmed ($expectedInferredSegsB)."
	}

	$emptyClonedFwdata = Join-Path $emptyOutDir $emptyResponse.materializedProjectPath
	$emptyProjectedOut = Join-Path $mpTempRoot 'empty-phoneme-inventory-projected'
	New-Item -ItemType Directory -Path $emptyProjectedOut -Force | Out-Null
	& $exePath project --project $emptyClonedFwdata --out-dir $emptyProjectedOut --database MPBase
	if ($LASTEXITCODE -ne 0) { Write-Error "mutate/parse live proof: 'project' on the empty-phoneme-inventory clone failed (exit $LASTEXITCODE)."; exit $LASTEXITCODE }
	Test-XampleFilesEquivalentIgnoringHvoRenumbering -BaseDir $baseProjectedOut -CloneDir $emptyProjectedOut -Database 'MPBase' -Label 'empty-phoneme-inventory'
	Assert-HcXmlLacksSegmentDefinition -BaseHcXmlPath (Join-Path $baseProjectedOut 'MPBase.hc.xml') -CloneHcXmlPath (Join-Path $emptyProjectedOut 'MPBase.hc.xml') -Representation 'k' -Label 'empty-phoneme-inventory'

	# --- Negative: a fixture with a SegmentNaturalClass, requireUnreferenced must refuse and delete nothing. ---
	$negativeCandidates = @('disjunctive-recheck', 'free-fluctuating-allomorph-pair', 'strrep-identity', 'diacritic-segments', 'loader-pattern-shapes')
	$negativeFixtureUsed = $null
	$negativeFwdata = $null
	foreach ($candidate in $negativeCandidates) {
		$candidateGrammar = Join-Path $mutateParseConformanceDir "edge-cases\$candidate\grammar.xml"
		if (-not (Test-Path $candidateGrammar)) {
			Write-Host "  negative-probe candidate '$candidate': fixture not found at $candidateGrammar, skipping."
			continue
		}
		$candidateOutDir = Join-Path $mpTempRoot "negative-author-$candidate"
		$candidateOutput = & $exePath author --grammar $candidateGrammar --out-dir $candidateOutDir --name NegProbe 2>&1
		if ($LASTEXITCODE -eq 0) {
			$negativeFixtureUsed = $candidate
			$negativeFwdata = Join-Path $candidateOutDir 'NegProbe\NegProbe.fwdata'
			Write-Host "  negative-probe candidate '$candidate': authored successfully, using it."
			break
		}
		Write-Host "  negative-probe candidate '$candidate': refused ($(($candidateOutput | Out-String).Trim())), trying next."
	}
	if (-not $negativeFixtureUsed) {
		Write-Error "mutate/parse live proof: none of the negative-probe candidate fixtures ($($negativeCandidates -join ', ')) could be authored."
		exit 1
	}

	$negativeInspectPath = Join-Path $mpTempRoot 'negative-inspect-before.json'
	& $exePath inspect --project $negativeFwdata --out $negativeInspectPath
	if ($LASTEXITCODE -ne 0) { Write-Error "mutate/parse live proof: negative-probe 'inspect' failed (exit $LASTEXITCODE)."; exit $LASTEXITCODE }
	$negativePhonemeCountBefore = (Get-Content $negativeInspectPath -Raw | ConvertFrom-Json).phonemes.Count

	$negativeSha256 = (Get-FileHash -Algorithm SHA256 -Path $negativeFwdata).Hash.ToLowerInvariant()
	$negativeRequestObj = @{
		schemaVersion = 1
		caseId        = 'negative-referenced-phoneme'
		baseSha256    = $negativeSha256
		operations    = @(@{ op = 'remove_all_phonemes'; requireUnreferenced = $true })
	}
	$negativeRequestPath = Join-Path $mpTempRoot 'negative-request.json'
	($negativeRequestObj | ConvertTo-Json -Depth 5) | Set-Content -Path $negativeRequestPath -Encoding utf8
	$negativeOutDir = Join-Path $mpTempRoot 'negative-out'
	Assert-ProjectorExit -ExePath $exePath -Label "negative probe ($negativeFixtureUsed)" -ExpectedExitCode 9 -ExpectedSubstrings 'mutation\.referenced-phoneme', 'PhNCSegments' -Arguments @(
		'mutate', '--project', $negativeFwdata, '--request', $negativeRequestPath, '--out-dir', $negativeOutDir
	) | Out-Null
	$negativeClonedFwdata = Join-Path $negativeOutDir 'NegProbe\NegProbe.fwdata'
	if (-not (Test-Path $negativeClonedFwdata)) { Write-Error "negative probe ($negativeFixtureUsed): refused but no clone was left at $negativeClonedFwdata"; exit 1 }
	$negativeInspectAfterPath = Join-Path $mpTempRoot 'negative-inspect-after.json'
	& $exePath inspect --project $negativeClonedFwdata --out $negativeInspectAfterPath
	if ($LASTEXITCODE -ne 0) { Write-Error "mutate/parse live proof: negative-probe clone 'inspect' failed (exit $LASTEXITCODE)."; exit $LASTEXITCODE }
	$negativePhonemeCountAfter = (Get-Content $negativeInspectAfterPath -Raw | ConvertFrom-Json).phonemes.Count
	if ($negativePhonemeCountAfter -ne $negativePhonemeCountBefore) {
		Write-Error "negative probe ($negativeFixtureUsed): phoneme count changed ($negativePhonemeCountBefore -> $negativePhonemeCountAfter) despite the refusal"
		exit 1
	}
	Write-Host "negative referenced-phoneme probe OK ($negativeFixtureUsed): exit 9, names mutation.referenced-phoneme + PhNCSegments, phoneme count unchanged ($negativePhonemeCountBefore)."

	# --- parse: the real XAMPLE engine over the base project's own generated files ---
	$mpWordsPath = Join-Path $mpTempRoot 'words.txt'
	Set-Content -Path $mpWordsPath -Value @('k', 'xxxxxxk') -Encoding utf8

	# The project's OWN adctl.txt must be provably untouched by a capped run: --max-prefixes
	# patches a COPY (ParseCommand.PatchedDynamicFilesDir), never the caller's file -- hash it
	# before and after rather than trusting that claim.
	$baseAdctlPath = Join-Path $baseProjectedOut 'MPBaseadctl.txt'
	$baseAdctlHashBeforeParse = (Get-FileHash -Algorithm SHA256 -Path $baseAdctlPath).Hash
	$baseParseOutPath = Join-Path $mpTempRoot 'base-parse.json'
	& $exePath parse --project $baseFwdata --project-dir $baseProjectedOut --database MPBase --words $mpWordsPath --out $baseParseOutPath --max-analyses 2000 --max-prefixes 12
	if ($LASTEXITCODE -ne 0) { Write-Error "mutate/parse live proof: base 'parse' failed (exit $LASTEXITCODE)."; exit $LASTEXITCODE }
	$baseAdctlHashAfterParse = (Get-FileHash -Algorithm SHA256 -Path $baseAdctlPath).Hash
	if ($baseAdctlHashAfterParse -ne $baseAdctlHashBeforeParse) {
		Write-Error "parse (--max-prefixes 12): the PROJECT'S OWN adctl.txt changed (sha256 $baseAdctlHashBeforeParse -> $baseAdctlHashAfterParse)."
		exit 1
	}
	& $exePath --validate-capture $baseParseOutPath
	if ($LASTEXITCODE -ne 0) { Write-Error "mutate/parse live proof: --validate-capture failed on base-parse.json (exit $LASTEXITCODE)."; exit $LASTEXITCODE }
	$baseParse = Get-Content $baseParseOutPath -Raw | ConvertFrom-Json
	if ($baseParse.parameters.adctlPatched -ne $true) { Write-Error "parse (--max-prefixes 12): expected parameters.adctlPatched true, got $($baseParse.parameters.adctlPatched)"; exit 1 }
	if ($baseParse.parameters.adctl.maxPrefixes -ne 12) { Write-Error "parse (--max-prefixes 12): expected parameters.adctl.maxPrefixes 12, got $($baseParse.parameters.adctl.maxPrefixes)"; exit 1 }
	Write-Host "parse adctl-untouched probe OK: base project's own adctl.txt unchanged (sha256 $baseAdctlHashBeforeParse); adctlPatched=true, adctl.maxPrefixes=12."

	# --- same base files, no cap override at all: adctlPatched must read false, adctlSource must
	#     name the project's own (unpatched) file, not a patched temp copy. ---
	$baseParseUnpatchedOutPath = Join-Path $mpTempRoot 'base-parse-unpatched.json'
	& $exePath parse --project $baseFwdata --project-dir $baseProjectedOut --database MPBase --words $mpWordsPath --out $baseParseUnpatchedOutPath --max-analyses 2000
	if ($LASTEXITCODE -ne 0) { Write-Error "mutate/parse live proof: base 'parse' (no cap overrides) failed (exit $LASTEXITCODE)."; exit $LASTEXITCODE }
	& $exePath --validate-capture $baseParseUnpatchedOutPath
	if ($LASTEXITCODE -ne 0) { Write-Error "mutate/parse live proof: --validate-capture failed on base-parse-unpatched.json (exit $LASTEXITCODE)."; exit $LASTEXITCODE }
	$baseParseUnpatched = Get-Content $baseParseUnpatchedOutPath -Raw | ConvertFrom-Json
	if ($baseParseUnpatched.parameters.adctlPatched -ne $false) { Write-Error "parse (no cap overrides): expected parameters.adctlPatched false, got $($baseParseUnpatched.parameters.adctlPatched)"; exit 1 }
	if ($baseParseUnpatched.parameters.adctlSource -match 'xample-projector-parse-') {
		Write-Error "parse (no cap overrides): adctlSource unexpectedly names a patched temp copy: $($baseParseUnpatched.parameters.adctlSource)"
		exit 1
	}
	Write-Host "parse adctlPatched=false probe OK: no cap overrides -> adctlPatched=false, adctlSource=$($baseParseUnpatched.parameters.adctlSource)."

	$kParse = $baseParse.words | Where-Object { $_.word -eq 'k' }
	$xxParse = $baseParse.words | Where-Object { $_.word -eq 'xxxxxxk' }
	if ($kParse.analyses.Count -ne 1) { Write-Error "parse: expected exactly 1 analysis for 'k', got $($kParse.analyses.Count)"; exit 1 }
	Write-Host "parse OK: engineVersion=$($baseParse.engineVersion), effective parameters: $($baseParse.parameters | ConvertTo-Json -Compress -Depth 5)"
	Write-Host "parse OK: 'k' = $($kParse.analyses.Count) analysis (analyses), reachedMaxAnalyses=$($kParse.reachedMaxAnalyses), engineError=$($kParse.engineError)"
	Write-Host "parse (reported as-is, never massaged): 'xxxxxxk' = $($xxParse.analyses.Count) analyses, reachedMaxAnalyses=$($xxParse.reachedMaxAnalyses), engineError=$($xxParse.engineError) (the HC oracle's own count for this word is 924 -- XAMPLE is a different engine and is not expected to match it)."

	# --- parse the remove-k-only clone's own files; multiset must match the base's. XAmple loads
	#     a FIXED, project-independent character table (cd.tab, under the FieldWorks install, not
	#     the project), never the project's own PhPhonemeSet, so a phoneme deletion changes
	#     nothing XAmple itself parses -- only HC's hc.xml load is sensitive to it (the
	#     InvalidShape warnings printed above come from 'project', not from this 'parse'). ---
	$removeKParseOutPath = Join-Path $mpTempRoot 'remove-k-only-parse.json'
	& $exePath parse --project $removeKClonedFwdata --project-dir $removeKProjectedOut --database MPBase --words $mpWordsPath --out $removeKParseOutPath --max-analyses 2000 --max-prefixes 12
	if ($LASTEXITCODE -ne 0) { Write-Error "mutate/parse live proof: remove-k-only clone 'parse' failed (exit $LASTEXITCODE)."; exit $LASTEXITCODE }
	$removeKParse = Get-Content $removeKParseOutPath -Raw | ConvertFrom-Json
	foreach ($word in @('k', 'xxxxxxk')) {
		$baseSigs = Get-AnalysisSignatures -ParseResponse $baseParse -Word $word
		$removeKSigs = Get-AnalysisSignatures -ParseResponse $removeKParse -Word $word
		if (($baseSigs -join '|') -ne ($removeKSigs -join '|')) {
			Write-Error "parse multiset comparison: '$word' differs between base ($($baseSigs.Count) analyses) and remove-k-only clone ($($removeKSigs.Count) analyses)."
			exit 1
		}
	}
	Write-Host "parse multiset comparison OK: base and remove-k-only clone produce identical analysis multisets for 'k' and 'xxxxxxk'."

	# --- parse the empty-phoneme-inventory clone's own files; multiset must match the base's ---
	$cloneParseOutPath = Join-Path $mpTempRoot 'clone-parse.json'
	& $exePath parse --project $emptyClonedFwdata --project-dir $emptyProjectedOut --database MPBase --words $mpWordsPath --out $cloneParseOutPath --max-analyses 2000 --max-prefixes 12
	if ($LASTEXITCODE -ne 0) { Write-Error "mutate/parse live proof: clone 'parse' failed (exit $LASTEXITCODE)."; exit $LASTEXITCODE }
	$cloneParse = Get-Content $cloneParseOutPath -Raw | ConvertFrom-Json
	foreach ($word in @('k', 'xxxxxxk')) {
		$baseSigs = Get-AnalysisSignatures -ParseResponse $baseParse -Word $word
		$cloneSigs = Get-AnalysisSignatures -ParseResponse $cloneParse -Word $word
		if (($baseSigs -join '|') -ne ($cloneSigs -join '|')) {
			Write-Error "parse multiset comparison: '$word' differs between base ($($baseSigs.Count) analyses) and empty-phoneme-inventory clone ($($cloneSigs.Count) analyses)."
			exit 1
		}
	}
	Write-Host "parse multiset comparison OK: base and empty-phoneme-inventory clone produce identical analysis multisets for 'k' ($($kParse.analyses.Count)) and 'xxxxxxk' ($($xxParse.analyses.Count))."
}
finally {
	Remove-Item -Path $mpTempRoot -Recurse -Force -ErrorAction SilentlyContinue
}

exit 0
