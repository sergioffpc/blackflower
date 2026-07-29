// Generates the trusted fixtures consumed by ../integration.rs.

#include <nanovdb/io/IO.h>
#include <nanovdb/tools/CreateNanoGrid.h>

#include <exception>
#include <iostream>
#include <string>

int main(int argc, char **argv)
{
    if (argc != 3) {
        std::cerr << "usage: generate_empty_grid <raw-output> <file-output>\n";
        return 2;
    }

    try {
        nanovdb::tools::build::Grid<float> source(
            3.25F,
            "density",
            nanovdb::GridClass::FogVolume);
        source.setTransform(0.5, nanovdb::Vec3d(1.0, 2.0, 3.0));
        auto handle = nanovdb::tools::createNanoGrid(
            source,
            nanovdb::tools::StatsMode::Default,
            nanovdb::CheckMode::Full);
        handle.write(std::string(argv[1]));
        nanovdb::io::writeGrid(
            std::string(argv[2]),
            handle,
            nanovdb::io::Codec::NONE);
        return 0;
    } catch (const std::exception &error) {
        std::cerr << error.what() << '\n';
        return 1;
    }
}
