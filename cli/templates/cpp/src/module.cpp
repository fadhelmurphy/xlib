#include "{{lib_name}}.hpp"

#include <numeric>
#include <stdexcept>

namespace {{lib_name}} {

int64_t add(int64_t a, int64_t b) noexcept { return a + b; }

std::string greet(const std::string& name) noexcept {
    return "Halo, " + name + "! (dari C++)";
}

double mean(const std::vector<double>& values) {
    if (values.empty()) throw std::invalid_argument("data kosong");
    return std::accumulate(values.begin(), values.end(), 0.0) / values.size();
}

std::vector<double> scale(const std::vector<double>& values, double factor) noexcept {
    std::vector<double> out;
    out.reserve(values.size());
    for (double v : values) out.push_back(v * factor);
    return out;
}

}  // namespace {{lib_name}}
