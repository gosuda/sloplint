#include <string>
std::string render_greeting(std::string name, bool loud);
std::string greet(std::string name, bool loud) { return render_greeting(name, loud); }
