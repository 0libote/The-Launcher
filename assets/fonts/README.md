# Embedded fonts

The UI bundles **Manrope Regular (400)** for body text and **Oxanium SemiBold (600)** for display text. Both use the SIL Open Font License 1.1; their unmodified licenses are included here. No fonts are downloaded at runtime.

The committed TTFs are static instances generated from official Google Fonts variable originals with `fontTools.varLib.instancer`. The originals default to weight 200, which is too light for this interface. Instantiation happens when preparing assets, not during the application build; Python and fontTools are not application dependencies.

| Asset | Official original | Original SHA-256 | Shipped weight |
| --- | --- | --- | --- |
| `Manrope.ttf` | [Manrope variable](https://raw.githubusercontent.com/google/fonts/main/ofl/manrope/Manrope%5Bwght%5D.ttf) | `3ae11c49db0455a3cc33e37d380f20fdb8c7f8b41dc07625c177e3d87a9d6ae6` | 400 |
| `Oxanium.ttf` | [Oxanium variable](https://raw.githubusercontent.com/google/fonts/main/ofl/oxanium/Oxanium%5Bwght%5D.ttf) | `2ce01d946e1e1ffc8d7eecfffbda8623bedd63eaf811a20488c4b69af45babb0` | 600 |

To regenerate, download the linked originals, verify their original hashes, and run:

```python
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont

for family, weight in [("Manrope", 400), ("Oxanium", 600)]:
    original = TTFont(f"{family}-variable.ttf")
    instance = instantiateVariableFont(original, {"wght": weight}, inplace=False)
    instance.save(f"{family}.ttf")
```

Retain [Manrope’s license](OFL-Manrope.txt) and [Oxanium’s license](OFL-Oxanium.txt) with redistributed binaries/assets. System and egui fallback fonts still cover glyphs absent from these Latin display families. Font choice alone does not guarantee full language coverage; test additional scripts before claiming localization support.
