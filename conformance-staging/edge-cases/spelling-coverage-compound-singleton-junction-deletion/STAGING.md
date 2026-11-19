# Singleton compound junction deletion

A deletion rule removes a stem-final segment before a stem-initial segment across a compound junction that writes a boundary. C# deletes through the junction (`p` is accepted as the compound `Z+Y`). The singleton FST route cannot reproduce the deletion across the written boundary, so it must refuse rather than compile and miss.

The verdict originates in PanGloss FST spelling admission. Signatures and rule traces come from C# HermitCrab at Machine `1ddef2bd2f1503717de95556116df8e4a63846d6` with suite `a0094a3f97ddaa4a4dc4c4718ba6ea44869e254d`. The fixture uses FieldWorks phoneme and boundary codes, compound rules and a phonological deletion rule. The word list holds the words C# accepts over the grammar's symbols.

Upstream PR
