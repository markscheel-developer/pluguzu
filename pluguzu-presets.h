// Pluguzu presets
#include <string>
#include <vector>

struct uzu_program {
  const char *name;
  const char *code;
  const std::vector<std::string> tags;
};

const uzu_program uzu_programs[] = {
  {"Calm", "stack [\n  n $ scale \"minor\" $ \"{[-12 .. -5]/4, [c4 f3 g3]}%<1 [1 2?]>\"\n, ccv (segment 32 $ range 0 128 $ sine) # ccn 1\n]\n", {"Demo"}},
  {"4 on the floor", "s(\"bd*4\")\n", {"Beat"}},
  {"Funky line", "n(\"0 0 - [- 0]\")\n", {"Bass"}},
  {"Secret Of Uzu", "-- todo\n", {"Melody"}},
};

const int uzu_presets_count = sizeof(uzu_programs) / sizeof(uzu_programs[0]);
