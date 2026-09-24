//! Canonical English FieldWorks navigation paths used in warning guidance.
//!
//! Tool labels and object locations were checked in FieldWorks' `Configuration` XML.

/// FieldWorks `Configuration/Lexicon/Edit/toolConfiguration.xml` labels this tool “Lexicon Edit”.
pub const LEXICON_EDIT: &str = "Lexicon > Lexicon Edit";
/// `Configuration/Grammar/Edit/toolConfiguration.xml` labels “Category Edit”; templates belong to a category.
/// `Configuration/strings-en.xml` names their object type “Inflectional Affix Template”.
pub const GRAMMAR_CATEGORY_AFFIX_TEMPLATES: &str =
    "Grammar > Category Edit > the category's Affix Templates";
/// `Configuration/Grammar/Edit/toolConfiguration.xml` labels this tool “Ad hoc Rules”.
pub const GRAMMAR_AD_HOC_RULES: &str = "Grammar > Ad hoc Rules";
/// `Configuration/Grammar/Edit/toolConfiguration.xml` and `strings-en.xml` label this “Compound Rules”.
pub const GRAMMAR_COMPOUND_RULES: &str = "Grammar > Compound Rules";
/// `Configuration/Grammar/Edit/toolConfiguration.xml` labels this tool “Category Edit”.
pub const GRAMMAR_CATEGORY_EDIT: &str = "Grammar > Category Edit";
/// `Configuration/Grammar/Edit/toolConfiguration.xml` and `strings-en.xml` label this “Phonemes”.
pub const GRAMMAR_PHONEMES: &str = "Grammar > Phonemes";
/// `Configuration/Grammar/Edit/toolConfiguration.xml` and `strings-en.xml` label this “Phonological Features”.
pub const GRAMMAR_PHONOLOGICAL_FEATURES: &str = "Grammar > Phonological Features";
/// `Configuration/Grammar/Edit/toolConfiguration.xml` labels this tool “Phonological Rules”.
/// `Grammar/areaConfiguration.xml` inserts both rule classes here; `strings-en.xml` supplies its plural label.
pub const GRAMMAR_PHONOLOGICAL_RULES: &str = "Grammar > Phonological Rules";
/// `Configuration/Grammar/Edit/toolConfiguration.xml` and `strings-en.xml` label this “Natural Classes”.
pub const GRAMMAR_NATURAL_CLASSES: &str = "Grammar > Natural Classes";
/// `Configuration/Grammar/Edit/toolConfiguration.xml` and `strings-en.xml` label this “Environments”.
pub const GRAMMAR_ENVIRONMENTS: &str = "Grammar > Environments";
/// `Configuration/Lists/Edit/toolConfiguration.xml` labels this tool “Variant Types”.
/// `Lists/areaConfiguration.xml` inserts `LexEntryInflType` there; `strings-en.xml` names “Variant Type”.
pub const LISTS_VARIANT_TYPES: &str = "Lists > Variant Types";
/// `Configuration/Words/areaConfiguration.xml` labels the command `_Edit Parser Parameters...`.
pub const WORDS_EDIT_PARSER_PARAMETERS: &str = "Words > Edit Parser Parameters...";
/// `Configuration/Main.xml` places this command under `Tools > Configure`.
pub const TOOLS_CONFIGURE_VERNACULAR_WRITING_SYSTEMS: &str =
    "Tools > Configure > Set up Vernacular Writing Systems...";
/// `Configuration/Main.xml` places this command under `Tools > Configure`.
pub const TOOLS_CONFIGURE_ANALYSIS_WRITING_SYSTEMS: &str =
    "Tools > Configure > Set up Analysis Writing Systems...";
/// `Configuration/Main.xml` labels the File-menu recovery command `_Restore a Project...`.
pub const FILE_RESTORE_PROJECT: &str = "File > Restore a Project...";
