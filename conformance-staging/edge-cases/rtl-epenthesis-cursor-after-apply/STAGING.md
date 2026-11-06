Review reproduction for a9a2a6c6875340e43dfd6547ab14a3dbe08ab6c5.

Source: Rust port regression. Pinned hc.dll at Machine 4d9b454d accepts
SUBRULE|uyu and rejects uyiu and uu. The base v0.16.0 binary agrees; reviewed
HEAD rejects uyu and admits SUBRULE|uyiu. Root uu, RTL iterative epenthesis,
subrule 0 inserts i between y and u, subrule 1 inserts y between u and u.
The new loop revisits the gap to the RIGHT of inserted y; C# resumes to its left.

This fixture remains staged pending an upstream conformance contribution; no
upstream Machine issue is warranted.
Raw oracle and Rust batch TSVs are in the enclosing a9a2a6c6-evidence directory.
