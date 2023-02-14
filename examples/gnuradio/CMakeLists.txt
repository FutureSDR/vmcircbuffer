project(NullRandFlowgraph)

cmake_minimum_required(VERSION 3.8)

find_package(Gnuradio "3.10" REQUIRED COMPONENTS blocks)

add_executable(copy_flowgraph copy_flowgraph.cpp)

target_link_libraries(copy_flowgraph
  gnuradio::gnuradio-runtime
  gnuradio::gnuradio-pmt
  gnuradio::gnuradio-blocks
)
