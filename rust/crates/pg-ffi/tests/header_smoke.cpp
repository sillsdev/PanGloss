#include "pangloss.h"
#include <type_traits>

static_assert(std::is_standard_layout<HcResultBuf>::value, "result buffer must be C layout");
static_assert(std::is_standard_layout<HcError>::value, "error must be C layout");
static_assert(std::is_standard_layout<HcStr>::value, "string view must be C layout");

extern "C" void pangloss_header_cpp_smoke() {
    auto load = &hc_grammar_load;
    auto parse = &hc_parse_word;
    auto batch = &hc_parse_batch;
    auto parse_opts = &hc_parse_word_opts;
    auto batch_opts = &hc_parse_batch_opts;
    auto generate = &hc_generate_words;
    auto json = &hc_lexicon_add_json;
    auto guide = &hc_classification_guide_answer_json;
    (void)load; (void)parse; (void)batch; (void)parse_opts; (void)batch_opts; (void)generate;
    (void)json; (void)guide;
}

int main() {
    pangloss_header_cpp_smoke();
    return 0;
}
