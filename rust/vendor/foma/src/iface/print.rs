//! foma/iface.c Wave-4 split: print/help/view commands and the help tables.
//! See iface/mod.rs.
use super::*;

/// C: `char warranty[]` — printed verbatim by iface_warranty.
static WARRANTY: &str = "\nLicensed under the Apache License, Version 2.0 (the \"License\")\nyou may not use this file except in compliance with the License.\nYou may obtain a copy of the License at\n\n    http://www.apache.org/licenses/LICENSE-2.0\n\n";

// [spec:foma:def:iface.global-help]
// C: struct global_help { char *name; char *help; char *longhelp; }.
pub struct GlobalHelp {
    pub name: &'static str,
    pub help: &'static str,
    pub longhelp: &'static str,
}

/// C: the file-static `struct global_help global_help[]` table (NULL-terminated).
/// The trailing `{NULL, NULL, NULL}` sentinel is represented by the slice end.
static GLOBAL_HELP: &[GlobalHelp] = &[
    GlobalHelp {
        name: "regex <regex>",
        help: "read a regular expression",
        longhelp: "Enter a regular expression and add result to top of stack.\nShort form: re\nSee `help operator' for operators, or `help precedence' for operator precedence.",
    },
    GlobalHelp {
        name: "ambiguous upper",
        help: "returns the input words which have multiple paths in a transducer",
        longhelp: "Short form: ambiguous\n",
    },
    GlobalHelp {
        name: "apply up <string>",
        help: "apply <string> up to the top network on stack",
        longhelp: "Short form: up <string>\n",
    },
    GlobalHelp {
        name: "apply down <string>",
        help: "apply <string> down to the top network on stack",
        longhelp: "Short form: down <string>\n",
    },
    GlobalHelp {
        name: "apply med <string>",
        help: "find approximate matches to string in top network by minimum edit distance",
        longhelp: "Short form: med <string>\n",
    },
    GlobalHelp {
        name: "apply up",
        help: "enter apply up mode (Ctrl-D exits)",
        longhelp: "Short form: up\n",
    },
    GlobalHelp {
        name: "apply down",
        help: "enter apply down mode (Ctrl-D exits)",
        longhelp: "Short form: down\n",
    },
    GlobalHelp {
        name: "apply med",
        help: "enter apply med mode (Ctrl-D exits)",
        longhelp: "Short form: med\n",
    },
    GlobalHelp {
        name: "apropos <string>",
        help: "search help for <string>",
        longhelp: "",
    },
    GlobalHelp {
        name: "clear stack",
        help: "clears the stack",
        longhelp: "",
    },
    GlobalHelp {
        name: "close sigma",
        help: "removes unknown symbols from FSM",
        longhelp: "",
    },
    GlobalHelp {
        name: "compact sigma",
        help: "removes redundant symbols from FSM",
        longhelp: "",
    },
    GlobalHelp {
        name: "complete net",
        help: "completes the FSM",
        longhelp: "",
    },
    GlobalHelp {
        name: "compose net",
        help: "composes networks on stack",
        longhelp: "",
    },
    GlobalHelp {
        name: "concatenate",
        help: "concatenates networks on stack",
        longhelp: "",
    },
    GlobalHelp {
        name: "crossproduct net",
        help: "cross-product of top two FSMs on stack",
        longhelp: "See ×\n",
    },
    GlobalHelp {
        name: "define <name> <r.e.>",
        help: "define a network",
        longhelp: "Example: \ndefine A x -> y;\n  and\nA = x -> y;\n\nare equivalent\n",
    },
    GlobalHelp {
        name: "define <fname>(<v1,..,vn>) <r.e.>",
        help: "define function",
        longhelp: "Example: define Remove(X) [X -> 0].l;",
    },
    GlobalHelp {
        name: "determinize net",
        help: "determinizes top FSM on stack",
        longhelp: "",
    },
    GlobalHelp {
        name: "echo <string>",
        help: "echo a string",
        longhelp: "",
    },
    GlobalHelp {
        name: "eliminate flag <name>",
        help: "eliminate flag <name> diacritics from the top network",
        longhelp: "",
    },
    GlobalHelp {
        name: "eliminate flags",
        help: "eliminate all flag diacritics from the top network",
        longhelp: "",
    },
    GlobalHelp {
        name: "export cmatrix (filename)",
        help: "export the confusion matrix as an AT&T transducer",
        longhelp: "",
    },
    GlobalHelp {
        name: "extract ambiguous",
        help: "extracts the ambiguous paths of a transducer",
        longhelp: "Short form: examb",
    },
    GlobalHelp {
        name: "extract unambiguous",
        help: "extracts the unambiguous paths of a transducer",
        longhelp: "Short form: exunamb",
    },
    GlobalHelp {
        name: "help license",
        help: "prints license",
        longhelp: "",
    },
    GlobalHelp {
        name: "help warranty",
        help: "prints warranty information",
        longhelp: "",
    },
    GlobalHelp {
        name: "ignore net",
        help: "applies ignore to top two FSMs on stack",
        longhelp: "See /\n",
    },
    GlobalHelp {
        name: "intersect net",
        help: "intersects FSMs on stack",
        longhelp: "See ∩ (or &)\n",
    },
    GlobalHelp {
        name: "invert net",
        help: "inverts top FSM",
        longhelp: "See ⁻¹ (or .i)\n",
    },
    GlobalHelp {
        name: "label net",
        help: "extracts all attested symbol pairs from FSM",
        longhelp: "See also: sigma net",
    },
    GlobalHelp {
        name: "letter machine",
        help: "Converts top FSM to a letter machine",
        longhelp: "See also: _lm(L)",
    },
    GlobalHelp {
        name: "load stack <filename>",
        help: "Loads networks and pushes them on the stack",
        longhelp: "Short form: load",
    },
    GlobalHelp {
        name: "load defined <filename>",
        help: "Restores defined networks from file",
        longhelp: "Short form: loadd",
    },
    GlobalHelp {
        name: "lower-side net",
        help: "takes lower projection of top FSM",
        longhelp: "See ₂ (or .l)\n",
    },
    GlobalHelp {
        name: "minimize net",
        help: "minimizes top FSM",
        longhelp: "Minimization can be controlled through the variable minimal: when set to OFF FSMs are never minimized.\nAlso, hopcroft-min can be set to OFF in which case minimization is done by double reversal and determinization (aka Brzozowski's algorithm).  It is likely to be much slower.\n",
    },
    GlobalHelp {
        name: "name net <string>",
        help: "names top FSM",
        longhelp: "",
    },
    GlobalHelp {
        name: "negate net",
        help: "complements top FSM",
        longhelp: "See ¬\n",
    },
    GlobalHelp {
        name: "one-plus net",
        help: "Kleene plus on top FSM",
        longhelp: "See +\n",
    },
    GlobalHelp {
        name: "pop stack",
        help: "remove top FSM from stack",
        longhelp: "",
    },
    GlobalHelp {
        name: "print cmatrix",
        help: "prints the confusion matrix associated with the top network in tabular format",
        longhelp: "",
    },
    GlobalHelp {
        name: "print defined",
        help: "prints defined symbols and functions",
        longhelp: "",
    },
    GlobalHelp {
        name: "print dot (>filename)",
        help: "prints top FSM in Graphviz dot format",
        longhelp: "",
    },
    GlobalHelp {
        name: "print lower-words",
        help: "prints words on the lower side of top FSM",
        longhelp: "",
    },
    GlobalHelp {
        name: "print lower-words > filename",
        help: "prints words on the lower side of top FSM to file",
        longhelp: "",
    },
    GlobalHelp {
        name: "print name",
        help: "prints the name of the top FSM",
        longhelp: "",
    },
    GlobalHelp {
        name: "print net",
        help: "prints all information about top FSM",
        longhelp: "Short form: net\n",
    },
    GlobalHelp {
        name: "print pairs",
        help: "prints input-output pairs from top FSM",
        longhelp: "Short form: pairs\n",
    },
    GlobalHelp {
        name: "print pairs > filename",
        help: "prints input-output pairs from top FSM to file",
        longhelp: "Short form: pairs\n",
    },
    GlobalHelp {
        name: "print random-lower",
        help: "prints random words from lower side",
        longhelp: "Short form: random-lower\n",
    },
    GlobalHelp {
        name: "print random-upper",
        help: "prints random words from upper side",
        longhelp: "Short form: random-upper",
    },
    GlobalHelp {
        name: "print random-words",
        help: "prints random words from top FSM",
        longhelp: "Short form: random-words\n",
    },
    GlobalHelp {
        name: "print random-pairs",
        help: "prints random input-output pairs from top FSM",
        longhelp: "Short form: random-pairs\n",
    },
    GlobalHelp {
        name: "print sigma",
        help: "prints the alphabet of the top FSM",
        longhelp: "Short form: sigma\n",
    },
    GlobalHelp {
        name: "print size",
        help: "prints size information about top FSM",
        longhelp: "Short form: size\n",
    },
    GlobalHelp {
        name: "print shortest-string",
        help: "prints the shortest string of the top FSM",
        longhelp: "Short form: pss\n",
    },
    GlobalHelp {
        name: "print shortest-string-size",
        help: "prints length of shortest string",
        longhelp: "Short form: psz\n",
    },
    GlobalHelp {
        name: "print upper-words",
        help: "prints words on the upper side of top FSM",
        longhelp: "Short form: upper-words",
    },
    GlobalHelp {
        name: "print upper-words > filename",
        help: "prints words on the upper side of top FSM to file",
        longhelp: "Short form:upper-words",
    },
    GlobalHelp {
        name: "print words",
        help: "prints words of top FSM",
        longhelp: "Short form: words",
    },
    GlobalHelp {
        name: "print words > filename",
        help: "prints words of top FSM to file",
        longhelp: "Short form: words",
    },
    GlobalHelp {
        name: "prune net",
        help: "makes top network coaccessible",
        longhelp: "",
    },
    GlobalHelp {
        name: "push (defined) <name>",
        help: "adds a defined FSM to top of stack",
        longhelp: "",
    },
    GlobalHelp {
        name: "quit",
        help: "exit foma",
        longhelp: "",
    },
    GlobalHelp {
        name: "read att <filename>",
        help: "read a file in AT&T FSM format and add to top of stack",
        longhelp: "Short form: ratt",
    },
    GlobalHelp {
        name: "read cmatrix <filename>",
        help: "read a confusion matrix and associate it with the network on top of the stack",
        longhelp: "",
    },
    GlobalHelp {
        name: "read prolog <filename>",
        help: "reads prolog format file",
        longhelp: "",
    },
    GlobalHelp {
        name: "read lexc <filename>",
        help: "read and compile lexc format file",
        longhelp: "",
    },
    GlobalHelp {
        name: "read spaced-text <filename>",
        help: "compile space-separated words/word-pairs separated by newlines into a FST",
        longhelp: "",
    },
    GlobalHelp {
        name: "read text <filename>",
        help: "compile a list of words separated by newlines into an automaton",
        longhelp: "",
    },
    GlobalHelp {
        name: "reverse net",
        help: "reverses top FSM",
        longhelp: "Short form: rev\nSee .r\n",
    },
    GlobalHelp {
        name: "rotate stack",
        help: "rotates stack",
        longhelp: "",
    },
    GlobalHelp {
        name: "save defined <filename>",
        help: "save all defined networks to binary file",
        longhelp: "Short form: saved",
    },
    GlobalHelp {
        name: "save stack <filename>",
        help: "save stack to binary file",
        longhelp: "Short form: ss",
    },
    GlobalHelp {
        name: "set <variable> <ON|OFF>",
        help: "sets a global variable (see show variables)",
        longhelp: "",
    },
    GlobalHelp {
        name: "show variables",
        help: "prints all variable/value pairs",
        longhelp: "",
    },
    GlobalHelp {
        name: "shuffle net",
        help: "asynchronous product on top two FSMs on stack",
        longhelp: "See ∥ (or <>)\n",
    },
    GlobalHelp {
        name: "sigma net",
        help: "Extracts the alphabet and creates a FSM that accepts all single symbols in it",
        longhelp: "See also: label net",
    },
    GlobalHelp {
        name: "source <file>",
        help: "read and compile script file",
        longhelp: "",
    },
    GlobalHelp {
        name: "sort net",
        help: "sorts arcs topologically on top FSM",
        longhelp: "",
    },
    GlobalHelp {
        name: "sort in",
        help: "sorts input arcs by sigma numbers on top FSM",
        longhelp: "",
    },
    GlobalHelp {
        name: "sort out",
        help: "sorts output arcs by sigma number on top FSM",
        longhelp: "",
    },
    GlobalHelp {
        name: "substitute defined X for Y",
        help: "substitutes defined network X at all arcs containing Y ",
        longhelp: "",
    },
    GlobalHelp {
        name: "substitute symbol X for Y",
        help: "substitutes all occurrences of Y in an arc with X",
        longhelp: "",
    },
    GlobalHelp {
        name: "system <cmd>",
        help: "execute a system command",
        longhelp: "",
    },
    GlobalHelp {
        name: "test unambiguous",
        help: "test if top FST is unambiguous",
        longhelp: "Short form: tunam\n",
    },
    GlobalHelp {
        name: "test equivalent",
        help: "test if the top two FSMs are equivalent",
        longhelp: "Short form: equ\nNote: equivalence is undecidable for transducers in the general case.  The result is reliable only for recognizers.\n",
    },
    GlobalHelp {
        name: "test functional",
        help: "test if the top FST is functional (single-valued)",
        longhelp: "Short form: tfu\n",
    },
    GlobalHelp {
        name: "test identity",
        help: "test if top FST represents identity relations only",
        longhelp: "Short form: tid\n",
    },
    GlobalHelp {
        name: "test lower-universal",
        help: "test if lower side is Σ*",
        longhelp: "Short form: tlu\n",
    },
    GlobalHelp {
        name: "test upper-universal",
        help: "test if upper side is Σ*",
        longhelp: "Short form: tuu\n",
    },
    GlobalHelp {
        name: "test non-null",
        help: "test if top machine is not the empty language",
        longhelp: "Short form:tnn\n",
    },
    GlobalHelp {
        name: "test null",
        help: "test if top machine is the empty language (∅)",
        longhelp: "Short form: tnu\n",
    },
    GlobalHelp {
        name: "test sequential",
        help: "tests if top machine is sequential",
        longhelp: "Short form: tseq\n",
    },
    GlobalHelp {
        name: "turn stack",
        help: "turns stack upside down",
        longhelp: "",
    },
    GlobalHelp {
        name: "twosided flag-diacritics",
        help: "changes flags to always be identity pairs",
        longhelp: "Short form: tfd",
    },
    GlobalHelp {
        name: "undefine <name>",
        help: "remove <name> from defined networks",
        longhelp: "See define\n",
    },
    GlobalHelp {
        name: "union net",
        help: "union of top two FSMs",
        longhelp: "See ∪ (or |)\n",
    },
    GlobalHelp {
        name: "upper-side net",
        help: "upper projection of top FSM",
        longhelp: "See ₁ (or .u)\n",
    },
    GlobalHelp {
        name: "view net",
        help: "display top network (if supported)",
        longhelp: "",
    },
    GlobalHelp {
        name: "zero-plus net",
        help: "Kleene star on top fsm",
        longhelp: "See *\n",
    },
    GlobalHelp {
        name: "variable compose-tristate",
        help: "use the tristate composition algorithm",
        longhelp: "Default value: OFF\n",
    },
    GlobalHelp {
        name: "variable show-flags",
        help: "show flag diacritics in `apply'",
        longhelp: "Default value: ON\n",
    },
    GlobalHelp {
        name: "variable obey-flags",
        help: "obey flag diacritics in `apply'",
        longhelp: "Default value: ON\n",
    },
    GlobalHelp {
        name: "variable minimal",
        help: "minimize resulting FSMs",
        longhelp: "Default value: ON\n",
    },
    GlobalHelp {
        name: "variable print-pairs",
        help: "always print both sides when applying",
        longhelp: "Default value: OFF\n",
    },
    GlobalHelp {
        name: "variable print-space",
        help: "print spaces between symbols",
        longhelp: "Default value: OFF\n",
    },
    GlobalHelp {
        name: "variable print-sigma",
        help: "print the alphabet when printing network",
        longhelp: "Default value: ON\n",
    },
    GlobalHelp {
        name: "quit-on-fail",
        help: "Abort operations when encountering errors",
        longhelp: "Default value: ON\n",
    },
    GlobalHelp {
        name: "variable recursive-define",
        help: "Allow recursive definitions",
        longhelp: "Default value: OFF\n",
    },
    GlobalHelp {
        name: "variable verbose",
        help: "Verbosity of interface",
        longhelp: "Default value: ON\n",
    },
    GlobalHelp {
        name: "variable hopcroft-min",
        help: "ON = Hopcroft minimization, OFF = Brzozowski minimization",
        longhelp: "Default value: ON\n",
    },
    GlobalHelp {
        name: "variable med-limit",
        help: "the limit on number of matches in apply med",
        longhelp: "Default value: 3\n",
    },
    GlobalHelp {
        name: "variable med-cutoff",
        help: "the cost limit for terminating a search in apply med",
        longhelp: "Default value: 3\n",
    },
    GlobalHelp {
        name: "variable att-epsilon",
        help: "the EPSILON symbol when reading/writing AT&T files",
        longhelp: "Default value: @0@\n",
    },
    GlobalHelp {
        name: "variable lexc-align",
        help: "Forces X:0 X:X of 0:X alignment of lexicon entry symbols",
        longhelp: "Default value: OFF\n",
    },
    GlobalHelp {
        name: "write prolog (> filename)",
        help: "writes top network to prolog format file/stdout",
        longhelp: "Short form: wpl",
    },
    GlobalHelp {
        name: "write att (> <filename>)",
        help: "writes top network to AT&T format file/stdout",
        longhelp: "Short form: watt",
    },
    GlobalHelp {
        name: "re operator: (∀<var name>)(F)",
        help: "universal quantification",
        longhelp: "Example: $.A is equivalent to:\n(∃x)(x ∈ A ∧ (∀y)(¬(y ∈ A ∧ ¬(x = y))))",
    },
    GlobalHelp {
        name: "re operator: (∃<var name>)(F)",
        help: "existential quantification",
        longhelp: "Example: $.A is equivalent to:\n(∃x)(x ∈ A ∧ ¬(∃y)(y ∈ A ∧ ¬(x = y)))",
    },
    GlobalHelp {
        name: "logic re operator: ∈",
        help: "`in' predicate for logical formulae",
        longhelp: "",
    },
    GlobalHelp {
        name: "logic re operator: S(t1,t2)",
        help: "successor-of predicate for logical formulae",
        longhelp: "",
    },
    GlobalHelp {
        name: "logic re operator: ≤",
        help: "less-than or equal-to",
        longhelp: "Refers to position of quantified substring\n",
    },
    GlobalHelp {
        name: "logic re operator: ≥",
        help: "more-than or equal-to",
        longhelp: "Refers to position of quantified substring\n",
    },
    GlobalHelp {
        name: "logic re operator: ≺",
        help: "precedes",
        longhelp: "Refers to position of quantified substring\n",
    },
    GlobalHelp {
        name: "logic re operator: ≻",
        help: "follows",
        longhelp: "Refers to position of quantified substring\n",
    },
    GlobalHelp {
        name: "logic re operator: ∧",
        help: "conjunction",
        longhelp: "Operationally equivalent to ∩\n",
    },
    GlobalHelp {
        name: "logic re operator: ∨",
        help: "disjunction",
        longhelp: "Operationally equivalent to ∪\n",
    },
    GlobalHelp {
        name: "logic re operator: →",
        help: "implication",
        longhelp: "A → B is equivalent to ¬A ∨ B ",
    },
    GlobalHelp {
        name: "logic re operator: ↔",
        help: "biconditional",
        longhelp: "A ↔ B is equivalent to (¬A ∨ B) ∧ (¬B ∨ A)",
    },
    GlobalHelp {
        name: "re operator: ∘ (or .o.) ",
        help: "compose",
        longhelp: "A .o. B is the composition of transducers/recognizers A and B\nThe composition algorithm can be controlled with the variable\ncompose-tristate.  The default algorithm is a `bistate' composition that eliminates redundant paths but may fail to find the shortest path.\n",
    },
    GlobalHelp {
        name: "re operator: × (or .x.) ",
        help: "cross-product",
        longhelp: "A × B (where A and B are recognizers, not transducers\nyields the cross-product of A and B.\n",
    },
    GlobalHelp {
        name: "re operator: .O. ",
        help: "`lenient' composition",
        longhelp: "Lenient composition as defined in Karttunen(1998)  A .O. B = [A ∘ B] .P. B\n",
    },
    GlobalHelp {
        name: "re operator: ∥ (or <>) ",
        help: "shuffle (asynchronous product)",
        longhelp: "A ∥ B yields the asynchronous (or shuffle) product of FSM A and B.\n",
    },
    GlobalHelp {
        name: "re operator: => ",
        help: "context restriction, e.g. A => B _ C, D _ E",
        longhelp: "A => B _ C yields the language where every instance of a substring drawn from A is surrounded by B and C.  Multiple contexts can be specified if separated by commas, e.g.: A => B _ C, D _ E",
    },
    GlobalHelp {
        name: "re operator: ->, <-, <->, etc.",
        help: "replacement operators",
        longhelp: "If LHS is a transducer, no RHS is needed in rule.",
    },
    GlobalHelp {
        name: "re operator: @->, @>, etc.",
        help: "directed replacement operators",
        longhelp: "",
    },
    GlobalHelp {
        name: "re operator: (->), (@->), etc. ",
        help: "optional replacements",
        longhelp: "Optional replacement operators variants.  Note that the directional modes leftmost/rightmost/longest/shortest are not affected by optionality, i.e. only replacement is optional, not mode.  Hence A (@->) B is not in general equivalent to the parallel rule A @-> B, A -> ... ",
    },
    GlobalHelp {
        name: "re operator: ||,\\/,\\\\,// ",
        help: "replacement direction specifiers",
        longhelp: "Rewrite rules direction specifier meaning is:\nA -> B || C _ D (replace if C and D match on upper side)\nA -> B // C _ D (replace if C matches of lower side and D matches on upper side)\nA -> B \\\\ C _ D (replace if C matches on upper side and D matches on lower side)\nA -> B \\/ C _ D (replace if C and D match on lower side)\n",
    },
    GlobalHelp {
        name: "re operator: _ ",
        help: "replacement or restriction context specifier",
        longhelp: "",
    },
    GlobalHelp {
        name: "re operator: ,,",
        help: "parallel context replacement operator",
        longhelp: "Separates parallel rules, e.g.:\nA -> B , C @-> D || E _ F ,, G -> H \\/ I _ J\n",
    },
    GlobalHelp {
        name: "re operator: ,",
        help: "parallel replacement operator",
        longhelp: "Separates rules and contexts. Example: A -> B, C <- D || E _ F",
    },
    GlobalHelp {
        name: "re operator: [.<r.e.>.]",
        help: "single-epsilon control in replacement LHS, e.g. [..] -> x",
        longhelp: "If the LHS contains the empty string, as does [.a*.] -> x, the rule yields a transducer where the empty string is assumed to occur exactly once between each symbol.",
    },
    GlobalHelp {
        name: "re operator: ...",
        help: "markup replacement control (e.g. A -> B ... C || D _ E)",
        longhelp: "A -> B ... C yields a replacement transducer where the center A is left untouched and B and C inserted around A.",
    },
    GlobalHelp {
        name: "re operator:  ",
        help: "concatenation",
        longhelp: "Binary operator: A B\nConcatenation is performed implicitly according to its precedence level without overt specification\n",
    },
    GlobalHelp {
        name: "re operator: ∪ (or |) ",
        help: "union",
        longhelp: "Binary operator: A|B",
    },
    GlobalHelp {
        name: "re operator: ∩ (or &) ",
        help: "intersection",
        longhelp: "Binary operator: A&B",
    },
    GlobalHelp {
        name: "re operator: - ",
        help: "set minus",
        longhelp: "Binary operator A-B",
    },
    GlobalHelp {
        name: "re operator: .P.",
        help: "priority union (upper)",
        longhelp: "Binary operator A .P. B\nEquivalent to: A .P. B = A ∪ [¬[A₁] ∘ B]\n",
    },
    GlobalHelp {
        name: "re operator: .p.",
        help: "priority union (lower)",
        longhelp: "Binary operator A .p. B\nEquivalent to: A .p. B = A ∪ [¬[A₂] ∘ B]",
    },
    GlobalHelp {
        name: "re operator: <",
        help: "precedes",
        longhelp: "Binary operator A < B\nYields the language where no instance of A follows an instance of B.",
    },
    GlobalHelp {
        name: "re operator: >",
        help: "follows",
        longhelp: "Binary operator A > B\nYields the language where no instance of A precedes an instance of B.",
    },
    GlobalHelp {
        name: "re operator: /",
        help: "ignore",
        longhelp: "Binary operator A/B\nYield the language/transducer where arbitrary sequences of strings/mappings from B are interspersed in A.  For single-symbol languages B, A/B = A ∥ B*",
    },
    GlobalHelp {
        name: "re operator: ./.",
        help: "ignore except at edges",
        longhelp: "Yields the language where arbitrary sequences from B are interspersed in A, except as the very first and very last symbol.",
    },
    GlobalHelp {
        name: "re operator: \\\\\\",
        help: "left quotient",
        longhelp: "Binary operator: A\\\\\\B\nInformally:  the set of suffixes one can add to A to get strings in B\n",
    },
    GlobalHelp {
        name: "re operator: ///",
        help: "right quotient",
        longhelp: "Binary operator A///B\nInformally: the set of prefixes one can add to B to get a string in A\n",
    },
    GlobalHelp {
        name: "re operator: /\\/",
        help: "interleaving quotient",
        longhelp: "Binary operator A/\\/B\nInformally: the set of strings you can interdigitate (non-continuously) to B to get strings in A\n",
    },
    GlobalHelp {
        name: "re operator: ¬ (or ~) ",
        help: "complement",
        longhelp: "Unary operator ~A, equivalent to Σ* - A\n",
    },
    GlobalHelp {
        name: "re operator: $",
        help: "contains a factor of",
        longhelp: "Unary operator $A\nEquivalent to: Σ* A Σ*\n",
    },
    GlobalHelp {
        name: "re operator: $.",
        help: "contains exactly one factor of",
        longhelp: "Unary operator $.A\nYields the language that contains exactly one factor from A.\nExample: if A = [a b|b a], $.A contains strings ab, ba, abb, bba, but not abab, baba, aba, bab, etc.\n",
    },
    GlobalHelp {
        name: "re operator: $?",
        help: "contains maximally one factor of",
        longhelp: "Unary operator: $?A, yields the language that contains zero or one factors from A. See also $.A.",
    },
    GlobalHelp {
        name: "re operator: +",
        help: "Kleene plus",
        longhelp: "Unary operator A+\n",
    },
    GlobalHelp {
        name: "re operator: *",
        help: "Kleene star",
        longhelp: "Unary operator A*\n",
    },
    GlobalHelp {
        name: "re operator: ^n ^<n ^>n ^{m,n}",
        help: "m, n-ary concatenations",
        longhelp: "A^n: A concatenated with itself exactly n times\nA^<n: A concatenated with itself less than n times\nA^>n: A concatenated with itself more than n times\nA^{m,n}: A concatenated with itself between m and n times\n",
    },
    GlobalHelp {
        name: "re operator: ₁ (or .1 or .u)",
        help: "upper projection",
        longhelp: "Unary operator A.u\n",
    },
    GlobalHelp {
        name: "re operator: ₂ (or .2 or .l)",
        help: "lower projection",
        longhelp: "Unary operator A.l\n",
    },
    GlobalHelp {
        name: "re operator: ⁻¹ (or .i)",
        help: "inverse of transducer",
        longhelp: "Unary operator A.i\n",
    },
    GlobalHelp {
        name: "re operator: .f",
        help: "eliminate all flags",
        longhelp: "Unary operator A.f: eliminates all flag diacritics in A",
    },
    GlobalHelp {
        name: "re operator: .r",
        help: "reverse of FSM",
        longhelp: "Unary operator A.r\n",
    },
    GlobalHelp {
        name: "re operator: :",
        help: "cross-product",
        longhelp: "Binary operator A:B, see also A × B\n",
    },
    GlobalHelp {
        name: "re operator: \\",
        help: "term complement (\\x = [Σ-x])",
        longhelp: "Unary operator \\A\nSingle symbols not in A.  Equivalent to [Σ-A]\n",
    },
    GlobalHelp {
        name: "re operator: `",
        help: "substitution/homomorphism",
        longhelp: "Ternary operator `[A,B,C] Replace instances of symbol B with symbol C in language A.  Also removes the substituted symbol from the alphabet.\n",
    },
    GlobalHelp {
        name: "re operator: { ... }",
        help: "concatenate symbols",
        longhelp: "Single-symbol-concatenation\nExample: {abcd} is equivalent to a b c d\n",
    },
    GlobalHelp {
        name: "re operator: (A)",
        help: "optionality",
        longhelp: "Equivalent to A | ε\nNote: parentheses inside logical formulas function as grouping, see ∀,∃\n",
    },
    GlobalHelp {
        name: "re operator: @\"filename\"",
        help: "read saved network from file",
        longhelp: "Note: loads networks stored with e.g. \"save stack\" but if file contains more than one network, only the first one is used in the regular expression.  See also \"load stack\" and \"load defined\"\n",
    },
    GlobalHelp {
        name: "special symbol: Σ (or ?)",
        help: "`any' symbol in r.e.",
        longhelp: "",
    },
    GlobalHelp {
        name: "special symbol: ε (or 0, [])",
        help: "epsilon symbol in r.e.",
        longhelp: "",
    },
    GlobalHelp {
        name: "special symbol: ∅",
        help: "the empty language symbol in r.e.",
        longhelp: "",
    },
    GlobalHelp {
        name: "special symbol: .#.",
        help: "word boundary symbol in replacements, restrictions",
        longhelp: "Signifies both end and beginning of word/string\nExample: A => B _ .#. (allow A only between B and end-of-string)\nExample: A -> B || .#. _ C (replace A with B if it occurs in the beginning of a word and is followed by C)\n",
    },
    GlobalHelp {
        name: "operator precedence: ",
        help: "see: `help precedence'",
        longhelp: "\\ `\n:\n+ * ^ ₁ ₂ ⁻¹ .f .r\n¬ $ $. $?\n(concatenation)\n> <\n∪ ∩ - .P. .p.\n=> -> (->) @-> etc.\n∥\n× ∘ .O.\nNote: compatibility variants (i.e. | = ∪ etc.) are not listed.",
    },
];

// [spec:foma:def:iface.iface-help-fn]
// [spec:foma:sem:iface.iface-help-fn]
// [spec:foma:def:foma.iface-help-fn]
// [spec:foma:sem:foma.iface-help-fn]
pub fn iface_help() {
    let maxlen = GLOBAL_HELP
        .iter()
        .map(|gh| gh.name.chars().count())
        .max()
        .unwrap_or(0);
    for gh in GLOBAL_HELP {
        // pad to maxlen + 1 columns so there is always at least one space
        let pad = " ".repeat(maxlen + 1 - gh.name.chars().count());
        println!("{}{}{}", gh.name, pad, gh.help);
    }
}

// [spec:foma:def:iface.iface-apropos-fn]
// [spec:foma:sem:iface.iface-apropos-fn]
// [spec:foma:def:foma.iface-apropos-fn]
// [spec:foma:sem:foma.iface-apropos-fn]
pub fn iface_apropos(s: &str) {
    // strstr(x, s) != NULL ↔ x contains s as a substring
    let matches = || {
        GLOBAL_HELP
            .iter()
            .filter(|gh| gh.name.contains(s) || gh.help.contains(s))
    };
    let maxlen = matches()
        .map(|gh| gh.name.chars().count())
        .max()
        .unwrap_or(0);
    for gh in matches() {
        let pad = " ".repeat(maxlen + 1 - gh.name.chars().count());
        println!("{}{}{}", gh.name, pad, gh.help);
    }
}

// [spec:foma:def:iface.iface-help-search-fn]
// [spec:foma:sem:iface.iface-help-search-fn]
// [spec:foma:def:foma.iface-help-search-fn]
// [spec:foma:sem:foma.iface-help-search-fn]
pub fn iface_help_search(s: &str) {
    for gh in GLOBAL_HELP {
        if gh.name.contains(s) || gh.help.contains(s) {
            println!("##");
            // printf("%-32.32s%s\n%s\n", name, help, longhelp): name is left-
            // justified and truncated/padded to exactly 32 BYTES (byte-based, not
            // UTF-8 aware), so the truncation is written as raw bytes.
            let nb = gh.name.as_bytes();
            let take = if nb.len() < 32 { nb.len() } else { 32 };
            let mut out = std::io::stdout();
            out.write_all(&nb[..take])
                .expect("write help output to stdout");
            for _ in take..32 {
                print!(" ");
            }
            print!("{}\n{}\n", gh.help, gh.longhelp);
        }
    }
}

// [spec:foma:def:iface.iface-print-bool-fn]
// [spec:foma:sem:iface.iface-print-bool-fn]
pub fn iface_print_bool(value: bool) {
    println!("{} (1 = TRUE, 0 = FALSE)", if value { 1 } else { 0 });
}

// [spec:foma:def:iface.iface-warranty-fn]
// [spec:foma:sem:iface.iface-warranty-fn]
// [spec:foma:def:foma.iface-warranty-fn]
// [spec:foma:sem:foma.iface-warranty-fn]
pub fn iface_warranty() {
    print!("{}", WARRANTY);
}

// [spec:foma:def:iface.iface-print-dot-fn]
// [spec:foma:sem:iface.iface-print-dot-fn]
// [spec:foma:def:foma.iface-print-dot-fn]
// [spec:foma:sem:foma.iface-print-dot-fn]
pub fn iface_print_dot(session: &mut Session, filename: Option<&str>) {
    if iface_stack_check(session, 1) {
        if let Some(f) = filename {
            println!("Writing dot file to {}.", f);
        }
        let Some(top) = session.stack_find_top() else {
            return;
        };
        session.stack_entry_fsm(top, |net| print_dot(net, filename));
    }
}

// [spec:foma:def:iface.iface-print-net-fn]
// [spec:foma:sem:iface.iface-print-net-fn]
// [spec:foma:def:foma.iface-print-net-fn]
// [spec:foma:sem:foma.iface-print-net-fn]
pub fn iface_print_net(session: &mut Session, netname: Option<&str>, filename: Option<&str>) {
    match netname {
        Some(netname) => {
            // net = find_defined(g_defines, netname)
            let found = match find_defined(&mut session.defines, netname) {
                Some(net) => {
                    print_net(net, filename);
                    true
                }
                None => false,
            };
            if !found && session.opts.verbose {
                eprintln!("No defined network {}.", netname);
                // fflush(stderr) — stderr is unbuffered
            }
        }
        None => {
            if iface_stack_check(session, 1) {
                let Some(top) = session.stack_find_top() else {
                    return;
                };
                session.stack_entry_fsm(top, |net| print_net(net, filename));
            }
        }
    }
}

// [spec:foma:def:iface.iface-print-cmatrix-att-fn]
// [spec:foma:sem:iface.iface-print-cmatrix-att-fn+1]
// [spec:foma:def:foma.iface-print-cmatrix-att-fn]
// [spec:foma:sem:foma.iface-print-cmatrix-att-fn+1]
pub fn iface_print_cmatrix_att(session: &mut Session, filename: Option<&str>) {
    if iface_stack_check(session, 1) {
        let Some(top) = session.stack_find_top() else {
            return;
        };
        let has_cm = session.stack_entry_fsm(top, |f| {
            // C: medlookup == NULL || medlookup->confusion_matrix == NULL. Empty Vec ↔ NULL.
            !f.medlookup
                .as_ref()
                .is_none_or(|m| m.confusion_matrix.is_empty())
        });
        if !has_cm {
            println!("No confusion matrix defined.");
        } else {
            match filename {
                None => {
                    session.stack_entry_fsm(top, |f| cmatrix_print_att(f, &mut std::io::stdout()));
                }
                Some(name) => {
                    // C: outfile = fopen(name,"w"); message; result NOT NULL-checked.
                    let res = File::create(name);
                    println!("Writing confusion matrix to file '{}'.", name);
                    // C's unchecked fopen NULL-derefs on failure; report the error
                    // and return instead of crashing, like the other file commands.
                    match res {
                        Ok(mut file) => {
                            session.stack_entry_fsm(top, |f| cmatrix_print_att(f, &mut file));
                            // C never fclose's the file (latent leak); Rust closes on drop.
                        }
                        Err(_) => {
                            eprint!("{}: ", name);
                            perror("Error opening output file.");
                        }
                    }
                }
            }
        }
    }
}

// [spec:foma:def:iface.iface-print-cmatrix-fn]
// [spec:foma:sem:iface.iface-print-cmatrix-fn]
// [spec:foma:def:foma.iface-print-cmatrix-fn]
// [spec:foma:sem:foma.iface-print-cmatrix-fn]
pub fn iface_print_cmatrix(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let Some(top) = session.stack_find_top() else {
            return;
        };
        let has_cm = session.stack_entry_fsm(top, |f| {
            // C: medlookup == NULL || medlookup->confusion_matrix == NULL. Empty Vec ↔ NULL.
            !f.medlookup
                .as_ref()
                .is_none_or(|m| m.confusion_matrix.is_empty())
        });
        if !has_cm {
            println!("No confusion matrix defined.");
        } else {
            session.stack_entry_fsm(top, |f| cmatrix_print(f));
        }
    }
}

// [spec:foma:def:iface.iface-print-defined-fn]
// [spec:foma:sem:iface.iface-print-defined-fn+1]
// [spec:foma:def:foma.iface-print-defined-fn]
// [spec:foma:sem:foma.iface-print-defined-fn+1]
pub fn iface_print_defined(session: &mut Session) {
    // C printed "No defined symbols.\n" only for a NULL g_defines (possible
    // only before main's init, never for a merely-empty registry); the session
    // registry always exists, so that branch is gone.
    let mut d = Some(&*session.defines);
    while let Some(node) = d {
        if let Some(name) = node.name.as_deref() {
            print!("{}\t", name);
            print_stats(
                node.net
                    .as_deref()
                    .expect("a named define always carries a net (name/net set together)"),
            );
        }
        d = node.next.as_deref();
    }
    let mut d = Some(&*session.defines_f);
    while let Some(node) = d {
        if let Some(name) = node.name.as_deref() {
            // Wave 4 fix: dropped the stray unmatched ')' from the C format
            // "%s@%i)\t" — now "%s@%i\t" (name@numargs then TAB).
            print!("{}@{}\t", name, node.numargs);
            println!("{}", node.regex.as_deref().unwrap_or(""));
        }
        d = node.next.as_deref();
    }
}

// [spec:foma:def:iface.iface-print-sigma-fn]
// [spec:foma:sem:iface.iface-print-sigma-fn]
// [spec:foma:def:foma.iface-print-sigma-fn]
// [spec:foma:sem:foma.iface-print-sigma-fn]
pub fn iface_print_sigma(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let Some(top) = session.stack_find_top() else {
            return;
        };
        session.stack_entry_fsm(top, |f| print_sigma(&f.sigma, &mut std::io::stdout()));
    }
}

// [spec:foma:def:iface.iface-print-stats-fn]
// [spec:foma:sem:iface.iface-print-stats-fn]
// [spec:foma:def:foma.iface-print-stats-fn]
// [spec:foma:sem:foma.iface-print-stats-fn]
pub fn iface_print_stats(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let Some(top) = session.stack_find_top() else {
            return;
        };
        session.stack_entry_fsm(top, |f| print_stats(f));
    }
}

// [spec:foma:def:iface.iface-view-fn]
// [spec:foma:sem:iface.iface-view-fn]
// [spec:foma:def:foma.iface-view-fn]
// [spec:foma:sem:foma.iface-view-fn]
pub fn iface_view(session: &mut Session) {
    if iface_stack_check(session, 1) {
        let Some(top) = session.stack_find_top() else {
            return;
        };
        session.stack_entry_fsm(top, view_net);
    }
}
