#!/usr/bin/env python3
"""Reproduce the two synthetic grammar-edit pairs from the pinned suite."""
import argparse
from pathlib import Path

EDITS = {
    # Changing a required slot to optional exercises morphology search.
    'optional-slot': ('cal-deep-template-16',
        '<Slot morphologicalRules="s01_pa s01_pi s01_ta s01_ti s01_ka s01_ki">',
        '<Slot optional="true" morphologicalRules="s01_pa s01_pi s01_ta s01_ti s01_ka s01_ki">'),
    # Remove four rules from the active cascade, leaving their definitions intact.
    'shorter-cascade': ('cal-rule-cascade-32',
        'phonologicalRules="ep1_i_s_Tvl su1_n_m su2_t_d su3_k_g su4_p_b ',
        'phonologicalRules="su4_p_b '),
}

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('edit', choices=EDITS)
parser.add_argument('suite', type=Path)
parser.add_argument('output', type=Path)
args = parser.parse_args()
family, before, after = EDITS[args.edit]
source = (args.suite / 'calibration' / family / 'grammar.xml').read_bytes()
old, new = before.encode(), after.encode()
if source.count(old) != 1:
    raise SystemExit('expected exactly one edit location in the pinned grammar')
args.output.write_bytes(source.replace(old, new, 1))
