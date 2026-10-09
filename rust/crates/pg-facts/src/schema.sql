CREATE TABLE artifact_meta (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    application_id INTEGER NOT NULL,
    schema_version INTEGER NOT NULL,
    format TEXT NOT NULL COLLATE BINARY,
    writer_version TEXT NOT NULL COLLATE BINARY,
    compiler_version TEXT NOT NULL COLLATE BINARY,
    source_revision TEXT NOT NULL COLLATE BINARY,
    build_identity TEXT NOT NULL COLLATE BINARY,
    snapshot_format TEXT NOT NULL COLLATE BINARY,
    snapshot_version INTEGER NOT NULL CHECK (snapshot_version >= 0),
    provenance_schema_version INTEGER NOT NULL CHECK (provenance_schema_version >= 0),
    source_inventory_status TEXT NOT NULL CHECK (source_inventory_status IN ('importedComplete', 'importedWithFatalIssues', 'synthetic', 'unknown')),
    source_sha256 TEXT NOT NULL COLLATE BINARY,
    grammar_hash TEXT NOT NULL COLLATE BINARY,
    model_fingerprint TEXT NOT NULL COLLATE BINARY,
    baseline_token_json TEXT NOT NULL COLLATE BINARY,
    baseline_key TEXT NOT NULL COLLATE BINARY,
    input_kind TEXT NOT NULL CHECK (input_kind IN ('baseline', 'proposal-dry-run')),
    dry_run_digest TEXT COLLATE BINARY,
    compile_options_json TEXT NOT NULL COLLATE BINARY,
    compile_options_sha256 TEXT NOT NULL COLLATE BINARY,
    compile_status TEXT NOT NULL CHECK (compile_status IN ('completed', 'refused')),
    complete INTEGER NOT NULL CHECK (complete IN (0, 1)),
    run_manifest_sha256 TEXT COLLATE BINARY,
    CHECK ((input_kind = 'baseline' AND dry_run_digest IS NULL) OR (input_kind = 'proposal-dry-run' AND dry_run_digest IS NOT NULL))
);

CREATE TABLE artifact_section (
    section TEXT PRIMARY KEY COLLATE BINARY CHECK (section IN ('project', 'source_census', 'conversion_inventory', 'categories', 'entries', 'msas', 'adhoc_prohibitions', 'load_accounting', 'effective_grammar', 'templates', 'allomorphs', 'environments', 'features', 'phonology', 'patterns', 'compound_rules', 'affix_processes', 'adhoc_groups', 'compiled_mappings', 'parser_config', 'stats')),
    status TEXT NOT NULL CHECK (status IN ('complete', 'partial', 'unavailable', 'not_requested')),
    source_scope TEXT NOT NULL COLLATE BINARY,
    reason_code TEXT COLLATE BINARY
);

CREATE TABLE project (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    name TEXT NOT NULL
);

CREATE TABLE writing_system (
    tag TEXT PRIMARY KEY COLLATE BINARY
);

CREATE TABLE project_writing_system (
    role TEXT NOT NULL COLLATE BINARY CHECK (role IN ('vernacular', 'analysis')),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    writing_system_tag TEXT NOT NULL COLLATE BINARY REFERENCES writing_system(tag),
    PRIMARY KEY (role, ordinal)
);

CREATE TABLE project_exemplar_character (
    ordinal INTEGER PRIMARY KEY CHECK (ordinal >= 0),
    character TEXT NOT NULL
);

CREATE TABLE category (
    guid TEXT PRIMARY KEY COLLATE BINARY,
    parent_guid TEXT COLLATE BINARY,
    sibling_ordinal INTEGER NOT NULL CHECK (sibling_ordinal >= 0),
    name TEXT NOT NULL,
    abbreviation TEXT NOT NULL,
    default_inflection_class_guid TEXT COLLATE BINARY
);

CREATE TABLE category_feature (
    category_guid TEXT NOT NULL COLLATE BINARY REFERENCES category(guid),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    feature_guid TEXT NOT NULL COLLATE BINARY,
    PRIMARY KEY (category_guid, ordinal)
);

CREATE TABLE inflection_class (
    guid TEXT PRIMARY KEY COLLATE BINARY,
    owner_category_guid TEXT NOT NULL COLLATE BINARY REFERENCES category(guid),
    parent_guid TEXT COLLATE BINARY,
    sibling_ordinal INTEGER NOT NULL CHECK (sibling_ordinal >= 0),
    name TEXT NOT NULL,
    abbreviation TEXT NOT NULL
);

-- Each category with itself (depth 0) and every ancestor, from category.parent_guid.
CREATE TABLE category_ancestor (
    category_guid TEXT NOT NULL COLLATE BINARY REFERENCES category(guid),
    ancestor_guid TEXT NOT NULL COLLATE BINARY REFERENCES category(guid),
    depth INTEGER NOT NULL CHECK (depth >= 0),
    PRIMARY KEY (category_guid, ancestor_guid),
    UNIQUE (category_guid, depth)
) WITHOUT ROWID;

CREATE INDEX category_ancestor_ancestor ON category_ancestor(ancestor_guid COLLATE BINARY);

-- Each inflection class with itself (depth 0) and every ancestor, from inflection_class.parent_guid.
CREATE TABLE inflection_class_ancestor (
    inflection_class_guid TEXT NOT NULL COLLATE BINARY REFERENCES inflection_class(guid),
    ancestor_guid TEXT NOT NULL COLLATE BINARY REFERENCES inflection_class(guid),
    depth INTEGER NOT NULL CHECK (depth >= 0),
    PRIMARY KEY (inflection_class_guid, ancestor_guid),
    UNIQUE (inflection_class_guid, depth)
) WITHOUT ROWID;

CREATE INDEX inflection_class_ancestor_ancestor ON inflection_class_ancestor(ancestor_guid COLLATE BINARY);

CREATE TABLE affix_slot (
    guid TEXT PRIMARY KEY COLLATE BINARY,
    category_guid TEXT NOT NULL COLLATE BINARY REFERENCES category(guid),
    name TEXT NOT NULL,
    optional INTEGER NOT NULL CHECK (optional IN (0, 1))
);

CREATE TABLE affix_template (
    guid TEXT PRIMARY KEY COLLATE BINARY,
    category_guid TEXT NOT NULL COLLATE BINARY REFERENCES category(guid),
    name TEXT NOT NULL,
    disabled INTEGER NOT NULL CHECK (disabled IN (0, 1)),
    is_final INTEGER NOT NULL CHECK (is_final IN (0, 1))
);

CREATE TABLE template_slot (
    template_guid TEXT NOT NULL COLLATE BINARY REFERENCES affix_template(guid),
    side TEXT NOT NULL COLLATE BINARY CHECK (side IN ('prefix', 'suffix')),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    slot_guid TEXT NOT NULL COLLATE BINARY,
    compiled_order INTEGER CHECK (compiled_order >= 0),
    surface_ordinal INTEGER,
    PRIMARY KEY (template_guid, side, ordinal),
    UNIQUE (template_guid, compiled_order)
);

CREATE TABLE lex_entry (
    guid TEXT PRIMARY KEY COLLATE BINARY,
    source_ordinal INTEGER NOT NULL CHECK (source_ordinal >= 0),
    lexeme_morph_type TEXT NOT NULL COLLATE BINARY
);

CREATE TABLE allomorph (
    guid TEXT PRIMARY KEY COLLATE BINARY,
    entry_guid TEXT NOT NULL COLLATE BINARY REFERENCES lex_entry(guid),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    morph_type TEXT NOT NULL COLLATE BINARY CHECK (morph_type IN ('stem', 'boundStem', 'root', 'boundRoot', 'prefix', 'suffix', 'infix', 'circumfix', 'proclitic', 'enclitic', 'clitic', 'particle', 'phrase', 'discontigPhrase', 'prefixingInterfix', 'infixingInterfix', 'suffixingInterfix')),
    form_class TEXT NOT NULL COLLATE BINARY CHECK (form_class IN ('stem', 'affix', 'process')),
    is_abstract INTEGER NOT NULL CHECK (is_abstract IN (0, 1)),
    stem_name_guid TEXT COLLATE BINARY,
    UNIQUE (entry_guid, ordinal)
);

CREATE TABLE allomorph_form (
    allomorph_guid TEXT NOT NULL COLLATE BINARY REFERENCES allomorph(guid),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    writing_system TEXT NOT NULL COLLATE BINARY REFERENCES writing_system(tag),
    form TEXT NOT NULL,
    PRIMARY KEY (allomorph_guid, ordinal)
);

-- One row per authored gate value. parser_effect is the compiler's outcome for that gate;
-- owner_not_loaded means no owner read the allomorph, so the compiler recorded nothing for it.
-- not_attempted means the compiler has no path that reads this gate on this allomorph kind.
CREATE TABLE allomorph_gate (
    allomorph_guid TEXT NOT NULL COLLATE BINARY REFERENCES allomorph(guid),
    gate_kind TEXT NOT NULL COLLATE BINARY CHECK (gate_kind IN ('inflection_class', 'required_features', 'required_category', 'stem_name')),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    target_guid TEXT COLLATE BINARY,
    fs_id INTEGER REFERENCES feature_structure(fs_id),
    parser_effect TEXT NOT NULL COLLATE BINARY CHECK (parser_effect IN ('applied', 'ignored', 'unresolved', 'owner_not_loaded', 'not_attempted')),
    reason_code TEXT COLLATE BINARY,
    PRIMARY KEY (allomorph_guid, gate_kind, ordinal),
    CHECK ((gate_kind = 'required_features') = (fs_id IS NOT NULL))
);
CREATE INDEX allomorph_gate_target ON allomorph_gate(target_guid);

CREATE TABLE entry_citation_form (
    entry_guid TEXT NOT NULL COLLATE BINARY REFERENCES lex_entry(guid),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    writing_system TEXT NOT NULL COLLATE BINARY,
    form TEXT NOT NULL,
    PRIMARY KEY (entry_guid, ordinal)
);

CREATE TABLE msa (
    msa_guid TEXT PRIMARY KEY COLLATE BINARY,
    entry_guid TEXT NOT NULL COLLATE BINARY REFERENCES lex_entry(guid),
    kind TEXT NOT NULL CHECK (kind IN ('stem', 'inflectional', 'derivational', 'unclassified'))
);

CREATE TABLE msa_category (
    msa_guid TEXT NOT NULL COLLATE BINARY REFERENCES msa(msa_guid),
    role TEXT NOT NULL COLLATE BINARY CHECK (role IN ('pos', 'from_pos', 'to_pos', 'clitic_from')),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    category_guid TEXT NOT NULL COLLATE BINARY,
    PRIMARY KEY (msa_guid, role, ordinal)
);

CREATE TABLE msa_slot (
    msa_guid TEXT NOT NULL COLLATE BINARY REFERENCES msa(msa_guid),
    role TEXT NOT NULL COLLATE BINARY CHECK (role IN ('slot', 'clitic_slot')),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    slot_guid TEXT NOT NULL COLLATE BINARY,
    PRIMARY KEY (msa_guid, role, ordinal)
);

CREATE TABLE msa_inflection_class (
    msa_guid TEXT NOT NULL COLLATE BINARY REFERENCES msa(msa_guid),
    role TEXT NOT NULL COLLATE BINARY CHECK (role IN ('class', 'from_class', 'to_class')),
    class_guid TEXT NOT NULL COLLATE BINARY,
    PRIMARY KEY (msa_guid, role)
);

CREATE TABLE msa_stem_name (
    msa_guid TEXT NOT NULL COLLATE BINARY REFERENCES msa(msa_guid),
    role TEXT NOT NULL COLLATE BINARY CHECK (role IN ('from_stem_name')),
    stem_name_guid TEXT NOT NULL COLLATE BINARY,
    PRIMARY KEY (msa_guid, role)
);

CREATE TABLE msa_exception_feature (
    msa_guid TEXT NOT NULL COLLATE BINARY REFERENCES msa(msa_guid),
    role TEXT NOT NULL COLLATE BINARY CHECK (role IN ('required', 'from_required', 'to_required')),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    target_guid TEXT NOT NULL COLLATE BINARY,
    PRIMARY KEY (msa_guid, role, ordinal)
);

CREATE TABLE sense (
    sense_guid TEXT PRIMARY KEY COLLATE BINARY,
    entry_guid TEXT NOT NULL COLLATE BINARY REFERENCES lex_entry(guid),
    msa_guid TEXT COLLATE BINARY
);

CREATE TABLE sense_text (
    sense_guid TEXT NOT NULL COLLATE BINARY REFERENCES sense(sense_guid),
    kind TEXT NOT NULL COLLATE BINARY CHECK (kind IN ('gloss', 'definition')),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    writing_system TEXT NOT NULL COLLATE BINARY,
    text TEXT NOT NULL,
    PRIMARY KEY (sense_guid, kind, ordinal)
) WITHOUT ROWID;

-- component_kind is 'unresolved' when the component is neither a loaded entry nor a sense.
CREATE TABLE entry_variant (
    variant_entry_guid TEXT NOT NULL COLLATE BINARY REFERENCES lex_entry(guid),
    ref_guid TEXT NOT NULL COLLATE BINARY,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    component_guid TEXT NOT NULL COLLATE BINARY,
    component_kind TEXT NOT NULL COLLATE BINARY CHECK (component_kind IN ('entry', 'sense', 'unresolved')),
    PRIMARY KEY (variant_entry_guid, ref_guid, ordinal)
);

-- Variant types live only here; is_infl_type is 1 when the type is a lex_entry_infl_type.
CREATE TABLE entry_variant_type (
    ref_guid TEXT NOT NULL COLLATE BINARY,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    type_guid TEXT NOT NULL COLLATE BINARY,
    is_infl_type INTEGER NOT NULL CHECK (is_infl_type IN (0, 1)),
    PRIMARY KEY (ref_guid, ordinal)
);

CREATE TABLE adhoc_prohibition (
    prohibition_guid TEXT PRIMARY KEY COLLATE BINARY,
    kind TEXT NOT NULL CHECK (kind IN ('allomorph', 'morpheme')),
    disabled INTEGER NOT NULL CHECK (disabled IN (0, 1)),
    adjacency TEXT NOT NULL CHECK (adjacency IN ('anywhere', 'somewhereToLeft', 'somewhereToRight', 'adjacentToLeft', 'adjacentToRight')),
    primary_guid TEXT NOT NULL COLLATE BINARY,
    target_kind TEXT NOT NULL CHECK (target_kind IN ('allomorph', 'msa'))
);

CREATE TABLE adhoc_other (
    prohibition_guid TEXT NOT NULL COLLATE BINARY REFERENCES adhoc_prohibition(prohibition_guid),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    target_guid TEXT NOT NULL COLLATE BINARY,
    target_kind TEXT NOT NULL CHECK (target_kind IN ('allomorph', 'msa')),
    PRIMARY KEY (prohibition_guid, ordinal)
);

CREATE TABLE adhoc_group (
    group_guid TEXT PRIMARY KEY COLLATE BINARY
);

CREATE TABLE adhoc_group_text (
    group_guid TEXT NOT NULL COLLATE BINARY REFERENCES adhoc_group(group_guid),
    field TEXT NOT NULL COLLATE BINARY CHECK (field IN ('name', 'description')),
    writing_system TEXT NOT NULL COLLATE BINARY REFERENCES writing_system(tag),
    text TEXT NOT NULL,
    PRIMARY KEY (group_guid, field, writing_system)
);

CREATE TABLE adhoc_group_member (
    group_guid TEXT NOT NULL COLLATE BINARY REFERENCES adhoc_group(group_guid),
    member_guid TEXT NOT NULL COLLATE BINARY,
    PRIMARY KEY (group_guid, member_guid)
);

CREATE TABLE source_census (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    total_occurrences INTEGER NOT NULL CHECK (total_occurrences >= 0),
    ordered_header_sha256 TEXT NOT NULL COLLATE BINARY
);

CREATE TABLE source_class_count (
    class_name TEXT PRIMARY KEY COLLATE BINARY,
    occurrences INTEGER NOT NULL CHECK (occurrences >= 0),
    unhandled_occurrences INTEGER NOT NULL CHECK (unhandled_occurrences >= 0)
);

CREATE TABLE source_object (
    source_key TEXT PRIMARY KEY COLLATE BINARY,
    source_ordinal INTEGER NOT NULL UNIQUE CHECK (source_ordinal >= 1),
    class_name TEXT NOT NULL COLLATE BINARY,
    raw_guid TEXT NOT NULL COLLATE BINARY,
    canonical_guid TEXT COLLATE BINARY,
    inventory_kind TEXT COLLATE BINARY CHECK (inventory_kind IN (
        'entry', 'sense', 'entryReference', 'msa', 'allomorph', 'affixProcess',
        'environment', 'phoneme', 'boundaryMarker', 'naturalClass', 'featureDefinition',
        'featureValue', 'featureStructure', 'phonologicalContext', 'featureConstraint',
        'morphemeCoOccurrence', 'allomorphCoOccurrence', 'phonologicalRule', 'compoundRule',
        'partOfSpeech', 'inflectionClass', 'stemName', 'ruleFeature', 'parserSetting',
        'strataConfiguration', 'template', 'templateSlot'
    )),
    handled INTEGER NOT NULL CHECK (handled IN (0, 1)),
    retained INTEGER NOT NULL CHECK (retained IN (0, 1)),
    duplicate INTEGER NOT NULL CHECK (duplicate IN (0, 1)),
    CHECK (canonical_guid IS NULL OR length(canonical_guid) = 36),
    CHECK (retained = 0 OR handled = 1)
) WITHOUT ROWID;

CREATE INDEX source_object_canonical_guid ON source_object(canonical_guid COLLATE BINARY);

CREATE TABLE conversion_item (
    pipeline_stage TEXT NOT NULL CHECK (pipeline_stage IN ('import', 'snapshot', 'compile', 'compact')),
    inventory_stage TEXT NOT NULL CHECK (inventory_stage IN ('authored', 'considered', 'selected', 'represented', 'rejected', 'synthesized')),
    subject_kind TEXT NOT NULL COLLATE BINARY,
    subject_key TEXT NOT NULL COLLATE BINARY,
    subject_guid TEXT COLLATE BINARY,
    PRIMARY KEY (pipeline_stage, inventory_stage, subject_kind, subject_key)
) WITHOUT ROWID;

CREATE TABLE conversion_stage (
    pipeline_stage TEXT NOT NULL CHECK (pipeline_stage IN ('import', 'snapshot', 'compile', 'compact')),
    inventory_stage TEXT NOT NULL CHECK (inventory_stage IN ('authored', 'considered', 'selected', 'represented', 'rejected', 'synthesized')),
    item_count INTEGER NOT NULL CHECK (item_count >= 0),
    PRIMARY KEY (pipeline_stage, inventory_stage)
);

CREATE TABLE conversion_issue (
    issue_key TEXT PRIMARY KEY COLLATE BINARY,
    pipeline_stage TEXT NOT NULL CHECK (pipeline_stage IN ('import', 'compile')),
    code TEXT NOT NULL COLLATE BINARY,
    issue_class TEXT NOT NULL COLLATE BINARY,
    fatal INTEGER NOT NULL CHECK (fatal IN (0, 1)),
    source_kind TEXT COLLATE BINARY,
    source_guid TEXT COLLATE BINARY,
    message TEXT NOT NULL
);

CREATE TABLE load_fact (
    subject_kind TEXT NOT NULL COLLATE BINARY,
    subject_key TEXT NOT NULL COLLATE BINARY,
    pipeline_stage TEXT NOT NULL CHECK (pipeline_stage IN ('import', 'snapshot', 'compile', 'compact')),
    decision_ordinal INTEGER NOT NULL CHECK (decision_ordinal >= 0),
    subject_guid TEXT COLLATE BINARY,
    context_key TEXT NOT NULL COLLATE BINARY,
    disposition TEXT NOT NULL CHECK (disposition IN ('represented', 'rejected', 'defaulted', 'synthesized', 'compacted', 'metadata_only', 'not_considered', 'unknown')),
    loaded INTEGER CHECK (loaded IN (0, 1) OR loaded IS NULL),
    reason_code TEXT NOT NULL COLLATE BINARY,
    effective_value_json TEXT COLLATE BINARY,
    issue_key TEXT COLLATE BINARY REFERENCES conversion_issue(issue_key),
    PRIMARY KEY (subject_kind, subject_key, pipeline_stage, context_key, decision_ordinal)
) WITHOUT ROWID;

CREATE INDEX load_fact_subject_guid ON load_fact(subject_guid COLLATE BINARY);

-- The final load state of each subject. Derived from load_fact by the rule in docs/grammar-facts-format.md.
CREATE TABLE object_state (
    subject_kind TEXT NOT NULL COLLATE BINARY,
    subject_guid TEXT NOT NULL COLLATE BINARY,
    final_stage TEXT NOT NULL COLLATE BINARY CHECK (final_stage IN ('import', 'snapshot', 'compile', 'compact')),
    loaded INTEGER CHECK (loaded IN (0, 1) OR loaded IS NULL),
    dispositions TEXT NOT NULL COLLATE BINARY,
    primary_reason TEXT NOT NULL COLLATE BINARY,
    PRIMARY KEY (subject_kind, subject_guid)
) WITHOUT ROWID;

CREATE INDEX object_state_guid ON object_state(subject_guid COLLATE BINARY);

-- One row per compiled grammar object. Lineage, allomorph order and stats all join on output_id; key
-- is the one spelling (canonical lowercase GUIDs, `!{n}` suffix on a collision). See docs/grammar-facts-format.md.
CREATE TABLE compiled_output (
    output_id INTEGER PRIMARY KEY CHECK (output_id > 0),
    kind TEXT NOT NULL COLLATE BINARY CHECK (kind IN ('lex_entry', 'morph_rule', 'allomorph', 'template', 'phon_rule', 'compound_rule', 'natural_class')),
    key TEXT NOT NULL COLLATE BINARY UNIQUE,
    owner_output_id INTEGER REFERENCES compiled_output(output_id),
    stratum_key TEXT COLLATE BINARY,
    bucket TEXT NOT NULL COLLATE BINARY,
    compiled_order INTEGER CHECK (compiled_order >= 0),
    identity_quality TEXT NOT NULL COLLATE BINARY CHECK (identity_quality IN ('authored', 'structural', 'synthetic')),
    realization_kind TEXT COLLATE BINARY CHECK (realization_kind IN ('segments', 'null', 'process', 'circumfix', 'infix', 'root')),
    has_phone_condition INTEGER CHECK (has_phone_condition IN (0, 1)),
    has_morph_gate INTEGER CHECK (has_morph_gate IN (0, 1)),
    gate_signature TEXT COLLATE BINARY,
    is_unconditioned INTEGER CHECK (is_unconditioned IN (0, 1)),
    CHECK ((kind = 'allomorph') = (compiled_order IS NOT NULL)),
    CHECK (kind = 'allomorph' OR (realization_kind IS NULL AND has_phone_condition IS NULL AND has_morph_gate IS NULL AND gate_signature IS NULL AND is_unconditioned IS NULL)),
    CHECK (kind <> 'allomorph' OR (realization_kind IS NOT NULL AND has_phone_condition IS NOT NULL AND has_morph_gate IS NOT NULL AND gate_signature IS NOT NULL AND is_unconditioned IS NOT NULL))
);

CREATE TABLE compiled_mapping (
    source_kind TEXT NOT NULL COLLATE BINARY,
    source_guid TEXT COLLATE BINARY,
    source_key TEXT NOT NULL COLLATE BINARY,
    output_id INTEGER NOT NULL REFERENCES compiled_output(output_id),
    relation_role TEXT NOT NULL COLLATE BINARY CHECK (relation_role IN ('form', 'circumfix_prefix_half', 'circumfix_suffix_half', 'null_affix', 'msa', 'entry', 'variant', 'rule')),
    source_ordinal INTEGER NOT NULL CHECK (source_ordinal >= 0),
    identity_quality TEXT NOT NULL CHECK (identity_quality IN ('authored', 'structural', 'synthetic')),
    PRIMARY KEY (source_kind, source_key, output_id, relation_role, source_ordinal)
) WITHOUT ROWID;

CREATE TABLE compiled_allomorph_order (
    owner_output_id INTEGER REFERENCES compiled_output(output_id),
    source_entry_guid TEXT COLLATE BINARY,
    source_msa_guid TEXT COLLATE BINARY,
    source_allomorph_key TEXT NOT NULL COLLATE BINARY,
    source_allomorph_guid TEXT COLLATE BINARY,
    output_id INTEGER REFERENCES compiled_output(output_id),
    compiled_order INTEGER CHECK (compiled_order >= 0),
    PRIMARY KEY (owner_output_id, source_allomorph_key, output_id)
);

CREATE INDEX compiled_allomorph_order_source ON compiled_allomorph_order(source_entry_guid, source_msa_guid);

-- The segmentation of each compiled form: a root's shape, or an affix's RHS in order. 'variable' is a
-- copy of the input, 'natural_class' a class insert, 'boundary' a morpheme or word boundary (the compiler's
-- synthetic morpheme boundary has no GUID and is identified by token_text), and 'synthetic' a segment with no source GUID.
CREATE TABLE compiled_form_segment (
    output_id INTEGER NOT NULL REFERENCES compiled_output(output_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    segment_kind TEXT NOT NULL COLLATE BINARY CHECK (segment_kind IN ('phoneme', 'boundary', 'natural_class', 'variable', 'synthetic')),
    phoneme_guid TEXT COLLATE BINARY REFERENCES phoneme(guid),
    boundary_guid TEXT COLLATE BINARY REFERENCES boundary_marker(guid),
    natural_class_guid TEXT COLLATE BINARY REFERENCES natural_class(guid),
    token_text TEXT,
    PRIMARY KEY (output_id, ordinal),
    CHECK ((segment_kind = 'phoneme') = (phoneme_guid IS NOT NULL)),
    CHECK (boundary_guid IS NULL OR segment_kind = 'boundary'),
    CHECK ((segment_kind = 'natural_class') = (natural_class_guid IS NOT NULL))
);

-- One frozen P7 cache identity and one declared run. Local dimension IDs below are rebuilt from
-- typed stable keys for deterministic facts output; original cache IDs remain only in provenance.
CREATE TABLE stats_cache_identity (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    cache_sha256 TEXT NOT NULL COLLATE BINARY,
    cache_bytes INTEGER NOT NULL CHECK (cache_bytes >= 0),
    schema_version INTEGER NOT NULL CHECK (schema_version > 0),
    counter_semantics INTEGER NOT NULL CHECK (counter_semantics > 0),
    engine TEXT NOT NULL COLLATE BINARY CHECK (engine = 'hc'),
    grammar_hash TEXT NOT NULL COLLATE BINARY
);

CREATE TABLE stats_run (
    run_id INTEGER PRIMARY KEY CHECK (run_id > 0),
    engine TEXT NOT NULL COLLATE BINARY CHECK (engine = 'hc'),
    grammar_hash TEXT NOT NULL COLLATE BINARY,
    options_hash TEXT NOT NULL COLLATE BINARY,
    options_json TEXT NOT NULL COLLATE BINARY,
    created_utc TEXT NOT NULL COLLATE BINARY,
    step_cap INTEGER CHECK (step_cap IS NULL OR step_cap = -1 OR step_cap > 0),
    cache_word_count INTEGER NOT NULL CHECK (cache_word_count >= 0),
    requested_word_count INTEGER NOT NULL CHECK (requested_word_count >= cache_word_count),
    batch_options_json TEXT NOT NULL COLLATE BINARY
);

CREATE TABLE stats_morpheme (
    morpheme_id INTEGER PRIMARY KEY CHECK (morpheme_id >= 0),
    key TEXT COLLATE BINARY,
    label TEXT,
    identity_quality TEXT CHECK (identity_quality IN ('authored', 'structural', 'synthetic')),
    CHECK ((morpheme_id = 0 AND key IS NULL AND identity_quality IS NULL) OR (morpheme_id > 0 AND key IS NOT NULL AND label IS NOT NULL AND identity_quality IS NOT NULL))
);

CREATE TABLE stats_stratum (
    stratum_id INTEGER PRIMARY KEY CHECK (stratum_id >= 0),
    key TEXT COLLATE BINARY,
    label TEXT,
    identity_quality TEXT CHECK (identity_quality IN ('authored', 'structural', 'synthetic')),
    CHECK ((stratum_id = 0 AND key IS NULL AND identity_quality IS NULL) OR (stratum_id > 0 AND key IS NOT NULL AND label IS NOT NULL AND identity_quality IS NOT NULL))
);

CREATE TABLE stats_allomorph (
    allomorph_id INTEGER PRIMARY KEY CHECK (allomorph_id >= 0),
    key TEXT COLLATE BINARY,
    label TEXT,
    identity_quality TEXT CHECK (identity_quality IN ('authored', 'structural', 'synthetic')),
    output_id INTEGER REFERENCES compiled_output(output_id),
    CHECK ((allomorph_id = 0 AND key IS NULL AND identity_quality IS NULL) OR (allomorph_id > 0 AND key IS NOT NULL AND label IS NOT NULL AND identity_quality IS NOT NULL))
);

CREATE TABLE stats_object (
    object_id INTEGER PRIMARY KEY CHECK (object_id > 0),
    key TEXT NOT NULL COLLATE BINARY,
    kind TEXT NOT NULL COLLATE BINARY CHECK (kind IN ('morph_rule', 'phon_rule', 'lex_entry', 'root_index', 'guesser', 'overlay')),
    label TEXT NOT NULL,
    identity_quality TEXT NOT NULL CHECK (identity_quality IN ('authored', 'structural', 'synthetic')),
    morpheme_id INTEGER NOT NULL REFERENCES stats_morpheme(morpheme_id),
    output_id INTEGER REFERENCES compiled_output(output_id),
    UNIQUE (kind, key)
);

CREATE TABLE stats_object_source (
    object_id INTEGER NOT NULL REFERENCES stats_object(object_id),
    source_kind TEXT NOT NULL COLLATE BINARY CHECK (source_kind IN ('entry', 'msa', 'phonologicalRule', 'compoundRule')),
    source_guid TEXT NOT NULL COLLATE BINARY,
    role TEXT NOT NULL COLLATE BINARY,
    PRIMARY KEY (object_id, source_kind, source_guid, role)
) WITHOUT ROWID;
CREATE INDEX stats_object_source_guid ON stats_object_source(source_kind, source_guid);

CREATE TABLE stats_allomorph_source (
    allomorph_id INTEGER NOT NULL REFERENCES stats_allomorph(allomorph_id),
    source_ordinal INTEGER NOT NULL CHECK (source_ordinal >= 0),
    source_allomorph_guid TEXT COLLATE BINARY,
    role TEXT NOT NULL COLLATE BINARY,
    omitted INTEGER NOT NULL CHECK (omitted IN (0, 1)),
    CHECK (omitted = 0 OR source_allomorph_guid IS NULL),
    PRIMARY KEY (allomorph_id, source_ordinal)
) WITHOUT ROWID;
CREATE INDEX stats_allomorph_source_guid ON stats_allomorph_source(source_allomorph_guid);

CREATE TABLE stats_word (
    word_id INTEGER PRIMARY KEY CHECK (word_id > 0),
    run_id INTEGER NOT NULL REFERENCES stats_run(run_id),
    form TEXT NOT NULL COLLATE BINARY,
    status TEXT NOT NULL CHECK (status IN ('complete', 'incomplete', 'invalid_shape', 'not_attempted')),
    elapsed_ns INTEGER CHECK (elapsed_ns IS NULL OR elapsed_ns >= 0),
    attempts INTEGER CHECK (attempts IS NULL OR attempts >= 0),
    passes INTEGER CHECK (passes IS NULL OR passes >= 0),
    capped INTEGER CHECK (capped IN (0, 1) OR capped IS NULL),
    timed_out INTEGER CHECK (timed_out IN (0, 1) OR timed_out IS NULL),
    invalid_shape INTEGER CHECK (invalid_shape IN (0, 1) OR invalid_shape IS NULL),
    UNIQUE (run_id, form),
    CHECK ((status = 'not_attempted' AND elapsed_ns IS NULL AND attempts IS NULL AND passes IS NULL AND capped IS NULL AND timed_out IS NULL AND invalid_shape IS NULL) OR (status <> 'not_attempted' AND elapsed_ns IS NOT NULL AND attempts IS NOT NULL AND passes IS NOT NULL AND capped IS NOT NULL AND timed_out IS NOT NULL AND invalid_shape IS NOT NULL))
);

CREATE TABLE stats_fact (
    word_id INTEGER NOT NULL REFERENCES stats_word(word_id),
    object_id INTEGER NOT NULL REFERENCES stats_object(object_id),
    stratum_id INTEGER NOT NULL REFERENCES stats_stratum(stratum_id),
    allomorph_id INTEGER NOT NULL REFERENCES stats_allomorph(allomorph_id),
    direction TEXT NOT NULL COLLATE BINARY CHECK (direction IN ('analysis', 'synthesis')),
    attempts INTEGER NOT NULL CHECK (attempts >= 0),
    work INTEGER NOT NULL CHECK (work >= 0),
    outputs INTEGER NOT NULL CHECK (outputs >= 0),
    not_applied INTEGER NOT NULL CHECK (not_applied >= 0),
    no_root INTEGER NOT NULL CHECK (no_root >= 0),
    surface_mismatch INTEGER NOT NULL CHECK (surface_mismatch >= 0),
    uses INTEGER NOT NULL CHECK (uses >= 0),
    self_time_ns INTEGER NOT NULL CHECK (self_time_ns >= 0),
    PRIMARY KEY (word_id, object_id, stratum_id, allomorph_id, direction)
) WITHOUT ROWID;
CREATE INDEX stats_fact_object ON stats_fact(object_id, direction);
CREATE INDEX stats_fact_stratum ON stats_fact(stratum_id, direction);
CREATE INDEX stats_fact_allomorph ON stats_fact(allomorph_id, direction);

-- The seven logical counters use the owner API. Timing is separate and records its current
-- direction-specific instrumentation support. This makes existing LexEntry/root-index/etc.
-- synthesis timing absence explicit instead of implying that zero time was measured.
CREATE TABLE stats_counter_support (
    engine TEXT NOT NULL COLLATE BINARY CHECK (engine = 'hc'),
    counter_semantics INTEGER NOT NULL CHECK (counter_semantics > 0),
    object_kind TEXT NOT NULL COLLATE BINARY CHECK (object_kind IN ('morph_rule', 'phon_rule', 'lex_entry', 'root_index', 'guesser', 'overlay')),
    counter TEXT NOT NULL COLLATE BINARY CHECK (counter IN ('attempts', 'work', 'outputs', 'not_applied', 'no_root', 'surface_mismatch', 'uses', 'self_time_ns')),
    direction TEXT NOT NULL COLLATE BINARY CHECK (direction IN ('both', 'analysis', 'synthesis')),
    support TEXT NOT NULL CHECK (support IN ('measured', 'not_applicable', 'not_wired')),
    PRIMARY KEY (engine, counter_semantics, object_kind, counter, direction),
    CHECK ((counter = 'self_time_ns' AND direction IN ('analysis', 'synthesis')) OR (counter <> 'self_time_ns' AND direction = 'both'))
) WITHOUT ROWID;

CREATE TABLE parser_config (
    setting_key TEXT PRIMARY KEY COLLATE BINARY,
    source_value_json TEXT COLLATE BINARY,
    effective_value_json TEXT NOT NULL COLLATE BINARY,
    source_presence TEXT NOT NULL CHECK (source_presence IN ('present', 'absent', 'not_retained'))
);

CREATE TABLE phoneme_set (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    guid TEXT COLLATE BINARY
);

CREATE TABLE phoneme (
    guid TEXT PRIMARY KEY COLLATE BINARY,
    feature_structure_id INTEGER REFERENCES feature_structure(fs_id),
    name TEXT NOT NULL,
    basic_ipa_symbol TEXT
);

CREATE TABLE phoneme_grapheme (
    phoneme_guid TEXT NOT NULL COLLATE BINARY REFERENCES phoneme(guid),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    writing_system TEXT NOT NULL COLLATE BINARY REFERENCES writing_system(tag),
    grapheme TEXT NOT NULL,
    PRIMARY KEY (phoneme_guid, ordinal)
);

CREATE TABLE boundary_marker (
    guid TEXT PRIMARY KEY COLLATE BINARY,
    name TEXT NOT NULL
);

CREATE TABLE boundary_grapheme (
    boundary_guid TEXT NOT NULL COLLATE BINARY REFERENCES boundary_marker(guid),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    writing_system TEXT NOT NULL COLLATE BINARY REFERENCES writing_system(tag),
    grapheme TEXT NOT NULL,
    PRIMARY KEY (boundary_guid, ordinal)
);

CREATE TABLE feature (
    guid TEXT PRIMARY KEY COLLATE BINARY,
    system TEXT NOT NULL CHECK (system IN ('phonological', 'morphosyntactic')),
    kind TEXT NOT NULL CHECK (kind IN ('closed', 'complex')),
    name TEXT NOT NULL,
    abbreviation TEXT NOT NULL,
    feature_type_guid TEXT COLLATE BINARY
);

CREATE TABLE feature_value (
    guid TEXT PRIMARY KEY COLLATE BINARY,
    feature_guid TEXT NOT NULL COLLATE BINARY REFERENCES feature(guid),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    name TEXT NOT NULL,
    abbreviation TEXT NOT NULL,
    UNIQUE (feature_guid, ordinal)
);

CREATE TABLE feature_structure (
    fs_id INTEGER PRIMARY KEY,
    system TEXT NOT NULL CHECK (system IN ('phonological', 'morphosyntactic')),
    owner_kind TEXT NOT NULL COLLATE BINARY,
    owner_guid TEXT NOT NULL COLLATE BINARY,
    role TEXT NOT NULL COLLATE BINARY,
    path TEXT NOT NULL COLLATE BINARY,
    UNIQUE (owner_kind, owner_guid, role, path)
);

CREATE TABLE feature_assignment (
    fs_id INTEGER NOT NULL REFERENCES feature_structure(fs_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    feature_guid TEXT NOT NULL COLLATE BINARY,
    value_kind TEXT NOT NULL CHECK (value_kind IN ('closed', 'complex')),
    value_guid TEXT COLLATE BINARY,
    child_fs_id INTEGER REFERENCES feature_structure(fs_id),
    PRIMARY KEY (fs_id, ordinal),
    CHECK ((value_kind = 'closed' AND value_guid IS NOT NULL AND child_fs_id IS NULL) OR (value_kind = 'complex' AND value_guid IS NULL AND child_fs_id IS NOT NULL))
);

CREATE TABLE natural_class (
    guid TEXT PRIMARY KEY COLLATE BINARY,
    kind TEXT NOT NULL CHECK (kind IN ('segments', 'features')),
    name TEXT NOT NULL,
    display_name TEXT,
    feature_structure_id INTEGER REFERENCES feature_structure(fs_id),
    CHECK ((kind = 'segments' AND feature_structure_id IS NULL) OR (kind = 'features' AND feature_structure_id IS NOT NULL))
);

CREATE TABLE natural_class_member (
    natural_class_guid TEXT NOT NULL COLLATE BINARY REFERENCES natural_class(guid),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    phoneme_guid TEXT NOT NULL COLLATE BINARY,
    PRIMARY KEY (natural_class_guid, ordinal)
);

CREATE TABLE natural_class_effective_member (
    natural_class_guid TEXT NOT NULL COLLATE BINARY REFERENCES natural_class(guid),
    table_key TEXT NOT NULL COLLATE BINARY,
    member_key TEXT NOT NULL COLLATE BINARY,
    phoneme_guid TEXT COLLATE BINARY,
    identity_quality TEXT NOT NULL CHECK (identity_quality IN ('sourceGuid', 'synthetic')),
    match_kind TEXT NOT NULL CHECK (match_kind IN ('segments', 'features')),
    -- 'underspecified': at least one feature matched only because its lane defaulted to the full mask.
    match_basis TEXT NOT NULL COLLATE BINARY CHECK (match_basis IN ('listed', 'specified', 'underspecified')),
    PRIMARY KEY (natural_class_guid, table_key, member_key)
);

CREATE TABLE feature_constraint (
    guid TEXT PRIMARY KEY COLLATE BINARY,
    feature_guid TEXT NOT NULL COLLATE BINARY
);

CREATE TABLE allomorph_environment (
    allomorph_guid TEXT NOT NULL COLLATE BINARY REFERENCES allomorph(guid),
    role TEXT NOT NULL CHECK (role IN ('phone', 'position')),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    environment_guid TEXT NOT NULL COLLATE BINARY,
    PRIMARY KEY (allomorph_guid, role, ordinal)
);

CREATE TABLE environment (
    guid TEXT PRIMARY KEY COLLATE BINARY,
    name TEXT NOT NULL,
    representation TEXT NOT NULL,
    parse_status TEXT NOT NULL CHECK (parse_status IN ('valid', 'invalid', 'not_attempted', 'unavailable')),
    parse_error_code TEXT COLLATE BINARY,
    parse_error_text TEXT,
    CHECK ((parse_status = 'invalid' AND parse_error_code IS NOT NULL AND parse_error_text IS NOT NULL) OR (parse_status <> 'invalid' AND parse_error_code IS NULL AND parse_error_text IS NULL))
);

CREATE TABLE environment_natural_class (
    environment_guid TEXT NOT NULL COLLATE BINARY REFERENCES environment(guid),
    compile_context_key TEXT NOT NULL COLLATE BINARY,
    side TEXT NOT NULL CHECK (side IN ('left', 'right')),
    token_path TEXT NOT NULL COLLATE BINARY,
    token_text TEXT NOT NULL,
    source_start INTEGER NOT NULL CHECK (source_start >= 0),
    source_end INTEGER NOT NULL CHECK (source_end >= source_start),
    natural_class_guid TEXT COLLATE BINARY REFERENCES natural_class(guid),
    result TEXT NOT NULL CHECK (result IN ('resolved', 'unresolved', 'not_attempted')),
    PRIMARY KEY (environment_guid, compile_context_key, side, token_path),
    CHECK ((result = 'resolved' AND natural_class_guid IS NOT NULL) OR (result <> 'resolved' AND natural_class_guid IS NULL))
);

-- One row per side of each valid environment. canonical_key is the lowercase hex SHA-256 of the side's
-- resolved token sequence with each class replaced by its sorted member keys; spacing and names never reach it.
CREATE TABLE environment_side (
    environment_guid TEXT NOT NULL COLLATE BINARY REFERENCES environment(guid),
    side TEXT NOT NULL COLLATE BINARY CHECK (side IN ('left', 'right')),
    canonical_key TEXT NOT NULL COLLATE BINARY,
    shape TEXT NOT NULL COLLATE BINARY CHECK (shape IN ('empty', 'single_segment', 'word_boundary', 'complex')),
    PRIMARY KEY (environment_guid, side)
);

-- Members of the sides whose shape is single_segment or word_boundary; a side's word boundary is a member too.
CREATE TABLE environment_side_member (
    environment_guid TEXT NOT NULL COLLATE BINARY,
    side TEXT NOT NULL COLLATE BINARY CHECK (side IN ('left', 'right')),
    member_kind TEXT NOT NULL COLLATE BINARY CHECK (member_kind IN ('phoneme', 'boundary', 'synthetic')),
    member_key TEXT NOT NULL COLLATE BINARY,
    phoneme_guid TEXT COLLATE BINARY REFERENCES phoneme(guid),
    boundary_guid TEXT COLLATE BINARY REFERENCES boundary_marker(guid),
    PRIMARY KEY (environment_guid, side, member_key),
    FOREIGN KEY (environment_guid, side) REFERENCES environment_side(environment_guid, side),
    CHECK ((member_kind = 'phoneme') = (phoneme_guid IS NOT NULL)),
    CHECK (boundary_guid IS NULL OR member_kind = 'boundary')
);

CREATE TABLE environment_usage (
    allomorph_guid TEXT NOT NULL COLLATE BINARY REFERENCES allomorph(guid),
    role TEXT NOT NULL CHECK (role IN ('phone', 'position')),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    compile_context_key TEXT NOT NULL COLLATE BINARY,
    environment_guid TEXT NOT NULL COLLATE BINARY,
    resolved_environment_guid TEXT COLLATE BINARY REFERENCES environment(guid),
    compiled INTEGER NOT NULL CHECK (compiled IN (0, 1)),
    result TEXT NOT NULL CHECK (result IN ('represented', 'invalid', 'unresolved', 'owner_not_loaded', 'not_attempted')),
    load_subject_kind TEXT COLLATE BINARY,
    load_subject_key TEXT COLLATE BINARY,
    load_pipeline_stage TEXT COLLATE BINARY,
    load_context_key TEXT COLLATE BINARY,
    load_decision_ordinal INTEGER CHECK (load_decision_ordinal >= 0),
    PRIMARY KEY (allomorph_guid, role, ordinal, compile_context_key),
    FOREIGN KEY (load_subject_kind, load_subject_key, load_pipeline_stage, load_context_key, load_decision_ordinal)
        REFERENCES load_fact(subject_kind, subject_key, pipeline_stage, context_key, decision_ordinal),
    CHECK ((compiled = 1 AND result = 'represented') OR (compiled = 0 AND result <> 'represented')),
    CHECK ((load_subject_kind IS NULL AND load_subject_key IS NULL AND load_pipeline_stage IS NULL AND load_context_key IS NULL AND load_decision_ordinal IS NULL) OR (load_subject_kind IS NOT NULL AND load_subject_key IS NOT NULL AND load_pipeline_stage IS NOT NULL AND load_context_key IS NOT NULL AND load_decision_ordinal IS NOT NULL))
);

CREATE TABLE phonological_rule (
    guid TEXT PRIMARY KEY COLLATE BINARY,
    name TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('rewrite', 'metathesis')),
    direction TEXT NOT NULL CHECK (direction IN ('leftToRight', 'rightToLeft', 'simultaneous')),
    order_index INTEGER NOT NULL CHECK (order_index >= 0),
    effective_stratum_key TEXT COLLATE BINARY REFERENCES stratum(stratum_key)
);

CREATE TABLE phonological_rule_variable (
    rule_guid TEXT NOT NULL COLLATE BINARY REFERENCES phonological_rule(guid),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    feature_constraint_guid TEXT NOT NULL COLLATE BINARY,
    PRIMARY KEY (rule_guid, ordinal)
);

CREATE TABLE rewrite_rhs (
    rule_guid TEXT NOT NULL COLLATE BINARY REFERENCES phonological_rule(guid),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    change_root_id INTEGER REFERENCES pattern_root(root_id),
    left_context_root_id INTEGER REFERENCES pattern_root(root_id),
    right_context_root_id INTEGER REFERENCES pattern_root(root_id),
    PRIMARY KEY (rule_guid, ordinal)
);

CREATE TABLE rewrite_rhs_pos (
    rule_guid TEXT NOT NULL COLLATE BINARY,
    rhs_ordinal INTEGER NOT NULL CHECK (rhs_ordinal >= 0),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    category_guid TEXT NOT NULL COLLATE BINARY,
    PRIMARY KEY (rule_guid, rhs_ordinal, ordinal),
    FOREIGN KEY (rule_guid, rhs_ordinal) REFERENCES rewrite_rhs(rule_guid, ordinal)
);

CREATE TABLE rewrite_rhs_rule_feature (
    rule_guid TEXT NOT NULL COLLATE BINARY,
    rhs_ordinal INTEGER NOT NULL CHECK (rhs_ordinal >= 0),
    polarity TEXT NOT NULL CHECK (polarity IN ('required', 'excluded')),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    target_guid TEXT NOT NULL COLLATE BINARY,
    target_kind TEXT NOT NULL CHECK (target_kind IN ('inflectionClass', 'exceptionFeature', 'featureValue', 'unresolved')),
    PRIMARY KEY (rule_guid, rhs_ordinal, polarity, ordinal),
    FOREIGN KEY (rule_guid, rhs_ordinal) REFERENCES rewrite_rhs(rule_guid, ordinal)
);

CREATE TABLE stratum (
    stratum_key TEXT PRIMARY KEY COLLATE BINARY,
    ordinal INTEGER NOT NULL UNIQUE CHECK (ordinal >= 0),
    name TEXT NOT NULL,
    table_key TEXT NOT NULL COLLATE BINARY
);

CREATE TABLE rule_stratum (
    rule_guid TEXT NOT NULL COLLATE BINARY REFERENCES phonological_rule(guid),
    stratum_key TEXT NOT NULL COLLATE BINARY REFERENCES stratum(stratum_key),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (rule_guid, stratum_key),
    UNIQUE (stratum_key, ordinal)
);

CREATE TABLE pattern_root (
    root_id INTEGER PRIMARY KEY,
    owner_kind TEXT NOT NULL COLLATE BINARY,
    owner_guid TEXT NOT NULL COLLATE BINARY,
    role TEXT NOT NULL COLLATE BINARY,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    source_kind TEXT NOT NULL CHECK (source_kind IN ('authored', 'resolved_environment')),
    UNIQUE (owner_kind, owner_guid, role, ordinal)
);

CREATE TABLE pattern_node (
    node_id INTEGER PRIMARY KEY,
    root_id INTEGER NOT NULL REFERENCES pattern_root(root_id),
    parent_node_id INTEGER,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    kind TEXT NOT NULL CHECK (kind IN ('sequence', 'iteration', 'quantifier', 'phoneme', 'naturalClass', 'boundary', 'wordBoundary', 'variable', 'characterDefinition', 'literalSegments', 'leftAnchor', 'rightAnchor')),
    min INTEGER CHECK (min >= 0),
    max INTEGER CHECK (max >= min),
    phoneme_guid TEXT COLLATE BINARY,
    natural_class_guid TEXT COLLATE BINARY,
    boundary_guid TEXT COLLATE BINARY,
    token_text TEXT,
    UNIQUE (root_id, node_id),
    FOREIGN KEY (root_id, parent_node_id) REFERENCES pattern_node(root_id, node_id),
    UNIQUE (root_id, parent_node_id, ordinal)
);

CREATE TABLE affix_process_input (
    allomorph_guid TEXT NOT NULL COLLATE BINARY REFERENCES allomorph(guid),
    part INTEGER NOT NULL CHECK (part >= 1),
    is_variable INTEGER NOT NULL CHECK (is_variable IN (0, 1)),
    pattern_root_id INTEGER REFERENCES pattern_root(root_id),
    PRIMARY KEY (allomorph_guid, part)
);

CREATE TABLE affix_process_output (
    allomorph_guid TEXT NOT NULL COLLATE BINARY REFERENCES allomorph(guid),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    kind TEXT NOT NULL CHECK (kind IN ('copy', 'insert_segments', 'insert_class', 'modify')),
    part INTEGER,
    natural_class_guid TEXT COLLATE BINARY,
    text TEXT,
    PRIMARY KEY (allomorph_guid, ordinal)
);

CREATE TABLE compound_rule (
    guid TEXT PRIMARY KEY COLLATE BINARY,
    kind TEXT NOT NULL CHECK (kind IN ('endocentric', 'exocentric')),
    name TEXT NOT NULL,
    disabled INTEGER NOT NULL CHECK (disabled IN (0, 1)),
    head_last INTEGER CHECK (head_last IN (0, 1)),
    max_applications INTEGER CHECK (max_applications >= 0)
);

CREATE TABLE compound_rule_side (
    rule_guid TEXT NOT NULL COLLATE BINARY REFERENCES compound_rule(guid),
    side TEXT NOT NULL CHECK (side IN ('left', 'right', 'outcome')),
    category_guid TEXT COLLATE BINARY,
    inflection_class_guid TEXT COLLATE BINARY,
    PRIMARY KEY (rule_guid, side)
);

CREATE TABLE compound_rule_exception_feature (
    rule_guid TEXT NOT NULL COLLATE BINARY REFERENCES compound_rule(guid),
    side TEXT NOT NULL CHECK (side IN ('left', 'right')),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    target_guid TEXT NOT NULL COLLATE BINARY,
    PRIMARY KEY (rule_guid, side, ordinal)
);

CREATE TABLE stem_name (
    guid TEXT PRIMARY KEY COLLATE BINARY,
    category_guid TEXT NOT NULL COLLATE BINARY,
    name TEXT NOT NULL,
    abbreviation TEXT,
    nonempty_region_count INTEGER NOT NULL CHECK (nonempty_region_count >= 0)
);

-- Each region is a feature_structure owned by ('stemName', stem guid, 'region'); its path is the region ordinal.
CREATE TABLE stem_name_region (
    stem_name_guid TEXT NOT NULL COLLATE BINARY REFERENCES stem_name(guid),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    fs_id INTEGER NOT NULL REFERENCES feature_structure(fs_id),
    PRIMARY KEY (stem_name_guid, ordinal)
);

CREATE TABLE exception_feature (
    guid TEXT PRIMARY KEY COLLATE BINARY,
    name TEXT NOT NULL,
    abbreviation TEXT NOT NULL
);

CREATE TABLE lex_entry_infl_type (
    guid TEXT PRIMARY KEY COLLATE BINARY,
    name TEXT NOT NULL,
    abbreviation TEXT NOT NULL,
    fs_id INTEGER REFERENCES feature_structure(fs_id)
);

CREATE TABLE lex_entry_infl_type_slot (
    infl_type_guid TEXT NOT NULL COLLATE BINARY REFERENCES lex_entry_infl_type(guid),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    slot_guid TEXT NOT NULL COLLATE BINARY,
    PRIMARY KEY (infl_type_guid, ordinal)
);

CREATE TABLE pattern_variable (
    node_id INTEGER NOT NULL REFERENCES pattern_node(node_id),
    polarity TEXT NOT NULL CHECK (polarity IN ('plus', 'minus')),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    feature_constraint_guid TEXT NOT NULL COLLATE BINARY,
    PRIMARY KEY (node_id, polarity, ordinal)
);

-- One row per parser-relevant reference to a grammar statement, keyed by target so "is X used, and
-- by what" is one index seek. Derived: it repeats facts other tables hold and adds the ones they do not.
-- parser_effect is the compiler's outcome for the referrer; see docs/grammar-facts-format.md.
CREATE TABLE statement_reference (
    target_kind TEXT NOT NULL COLLATE BINARY CHECK (target_kind IN ('environment', 'naturalClass', 'phoneme', 'boundary', 'category', 'inflectionClass', 'slot', 'exceptionFeature', 'feature', 'featureValue', 'featureConstraint', 'stemName', 'msa', 'allomorph', 'template', 'entry', 'sense', 'inflType', 'variantType', 'mprFeature')),
    target_guid TEXT NOT NULL COLLATE BINARY,
    referrer_kind TEXT NOT NULL COLLATE BINARY CHECK (referrer_kind IN ('allomorph', 'environment', 'affixProcess', 'phonologicalRule', 'compoundRule', 'msa', 'template', 'inflType', 'adhocProhibition', 'naturalClass', 'phoneme', 'category', 'stemName', 'featureConstraint', 'sense', 'entry')),
    referrer_guid TEXT NOT NULL COLLATE BINARY,
    role TEXT NOT NULL COLLATE BINARY CHECK (role IN ('phone_env', 'position_env', 'inflection_class', 'required_category', 'stem_name', 'required_features', 'env_token', 'env_segment', 'process_input', 'process_insert', 'process_modify', 'rewrite_lhs', 'rewrite_sc', 'rewrite_left_context', 'rewrite_right_context', 'metathesis_pattern', 'rewrite_pos', 'rule_feature_required', 'rule_feature_excluded', 'rule_variable', 'pattern_variable', 'left_category', 'left_exception', 'right_category', 'right_exception', 'outcome_category', 'outcome_class', 'pos', 'from_pos', 'to_pos', 'clitic_from', 'class', 'from_class', 'to_class', 'from_stem_name', 'slot', 'clitic_slot', 'required', 'from_required', 'to_required', 'features', 'from_features', 'to_features', 'prefix_slot', 'suffix_slot', 'primary', 'other', 'default_class', 'inflectable_feature', 'template', 'region', 'constraint_feature', 'msa', 'variant_component', 'variant_type', 'class_member', 'class_effective_member', 'form_segment', 'via_ancestor')),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    parser_effect TEXT NOT NULL COLLATE BINARY CHECK (parser_effect IN ('applied', 'ignored', 'unresolved', 'owner_not_loaded', 'not_attempted')),
    PRIMARY KEY (target_kind, target_guid, referrer_kind, referrer_guid, role, ordinal)
) WITHOUT ROWID;

-- Reverse lookups from a referenced object to the rows that name it, for drill-down views.
CREATE INDEX allomorph_environment_environment ON allomorph_environment(environment_guid COLLATE BINARY);
CREATE INDEX environment_usage_environment ON environment_usage(environment_guid COLLATE BINARY);
CREATE INDEX environment_natural_class_class ON environment_natural_class(natural_class_guid COLLATE BINARY);
CREATE INDEX pattern_node_natural_class ON pattern_node(natural_class_guid COLLATE BINARY);
CREATE INDEX pattern_node_phoneme ON pattern_node(phoneme_guid COLLATE BINARY);
CREATE INDEX msa_slot_slot ON msa_slot(slot_guid COLLATE BINARY);
CREATE INDEX msa_category_category ON msa_category(category_guid COLLATE BINARY);
CREATE INDEX msa_entry ON msa(entry_guid COLLATE BINARY);
CREATE INDEX template_slot_slot ON template_slot(slot_guid COLLATE BINARY);
CREATE INDEX compiled_mapping_source ON compiled_mapping(source_guid COLLATE BINARY);
CREATE INDEX compiled_allomorph_order_source_allomorph ON compiled_allomorph_order(source_allomorph_guid COLLATE BINARY);
CREATE INDEX sense_msa ON sense(msa_guid COLLATE BINARY);
CREATE INDEX adhoc_other_target ON adhoc_other(target_guid COLLATE BINARY);
