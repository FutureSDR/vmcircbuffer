#include <algorithm>
#include <boost/format.hpp>
#include <chrono>
#include <functional>
#include <iostream>
#include <numeric>
#include <random>

#include <gnuradio/blocks/copy.h>
#include <gnuradio/blocks/vector_sink.h>
#include <gnuradio/blocks/vector_source.h>
#include <gnuradio/top_block.h>

using namespace gr;

float get_random() {
    static std::default_random_engine e;
    static std::uniform_real_distribution<> dis(0, 1); // rage 0 - 1
    return dis(e);
}


int main(int argc, char **argv) {
    int n_copy = 200;
    uint64_t n_samples = 20000000;

    std::vector<float> vec;
    for (int i = 0; i != n_samples; i++) {
        vec.emplace_back(get_random());
    }

    auto tb = gr::make_top_block("copy");

    auto src = blocks::vector_source_f::make(vec);
    auto prev = blocks::copy::make(sizeof(float));
    tb->connect(src, 0, prev, 0);

    for (int stage = 1; stage < n_copy; stage++) {
        auto block = blocks::copy::make(sizeof(float));
        tb->connect(prev, 0, block, 0);
        prev = block;
    }

    auto sink = blocks::vector_sink_f::make(1, n_samples);
    tb->connect(prev, 0, sink, 0);

    auto start = std::chrono::high_resolution_clock::now();
    tb->run();
    auto finish = std::chrono::high_resolution_clock::now();
    auto time =
            std::chrono::duration_cast<std::chrono::nanoseconds>(finish - start)
                    .count() /
            1e9;

    std::cout << boost::format("%1$20.15f") % time << std::endl;

    return 0;
}
