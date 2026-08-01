#include <openvdb/io/File.h>
#include <openvdb/openvdb.h>

#include <nanovdb/GridHandle.h>
#include <nanovdb/io/IO.h>
#include <nanovdb/tools/CreateNanoGrid.h>
#include <nanovdb/tools/GridValidator.h>

#include <algorithm>
#include <cstdint>
#include <exception>
#include <filesystem>
#include <iostream>
#include <map>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

namespace {

struct Arguments {
    std::filesystem::path input;
    std::filesystem::path output;
    std::vector<std::string> grids;
};

Arguments parse_arguments(int argc, char **argv)
{
    Arguments arguments;
    for (int index = 1; index < argc; ++index) {
        const std::string flag(argv[index]);
        if (flag == "--input" || flag == "--output" || flag == "--grid") {
            if (++index >= argc) {
                throw std::runtime_error("missing value after " + flag);
            }
            if (flag == "--input") {
                if (!arguments.input.empty()) {
                    throw std::runtime_error("--input was specified more than once");
                }
                arguments.input = std::filesystem::path(argv[index]);
            } else if (flag == "--output") {
                if (!arguments.output.empty()) {
                    throw std::runtime_error("--output was specified more than once");
                }
                arguments.output = std::filesystem::path(argv[index]);
            } else {
                arguments.grids.emplace_back(argv[index]);
            }
        } else {
            throw std::runtime_error("unsupported argument: " + flag);
        }
    }
    if (arguments.input.empty() || arguments.output.empty() || arguments.grids.empty()) {
        throw std::runtime_error("--input, --output, and at least one --grid are required");
    }
    if (!std::is_sorted(arguments.grids.begin(), arguments.grids.end()) ||
        std::adjacent_find(arguments.grids.begin(), arguments.grids.end()) !=
            arguments.grids.end()) {
        throw std::runtime_error("grid names must be strictly sorted and unique");
    }
    if (std::filesystem::exists(arguments.output)) {
        throw std::runtime_error("output path already exists");
    }
    return arguments;
}

std::map<std::string, std::uint32_t> grid_name_counts(const openvdb::io::File &file)
{
    std::map<std::string, std::uint32_t> counts;
    for (auto iterator = file.beginName(); iterator != file.endName(); ++iterator) {
        ++counts[*iterator];
    }
    return counts;
}

nanovdb::GridHandle<nanovdb::HostBuffer> convert_grid(
    const openvdb::GridBase::Ptr &source,
    const std::string &expected_name)
{
    auto handle = nanovdb::tools::openToNanoVDB(
        source,
        nanovdb::tools::StatsMode::BBox,
        nanovdb::CheckMode::Full,
        0);
    if (!handle || handle.gridCount() != 1) {
        throw std::runtime_error(
            "grid `" + expected_name + "` has unsupported OpenVDB type `" +
            source->type() + "`");
    }
    const auto *metadata = handle.gridMetaData();
    if (metadata == nullptr ||
        std::string(metadata->shortGridName()) != expected_name) {
        throw std::runtime_error(
            "grid `" + expected_name + "` changed identity during conversion");
    }
    if (!nanovdb::tools::validateGrids(
            handle,
            nanovdb::CheckMode::Full,
            false)) {
        throw std::runtime_error(
            "grid `" + expected_name + "` failed full NanoVDB validation");
    }
    return handle;
}

void cook(const Arguments &arguments)
{
    openvdb::initialize();
    openvdb::io::File file(arguments.input.string());
    file.open(false);
    const auto counts = grid_name_counts(file);
    std::vector<nanovdb::GridHandle<nanovdb::HostBuffer>> handles;
    handles.reserve(arguments.grids.size());
    for (const auto &name : arguments.grids) {
        const auto found = counts.find(name);
        if (found == counts.end()) {
            throw std::runtime_error("selected grid `" + name + "` does not exist");
        }
        if (found->second != 1) {
            throw std::runtime_error("selected grid name `" + name + "` is ambiguous");
        }
        handles.push_back(convert_grid(file.readGrid(name), name));
    }
    file.close();

    auto merged =
        nanovdb::mergeGrids<nanovdb::HostBuffer, std::vector>(handles);
    if (!merged || !nanovdb::tools::validateGrids(
                       merged,
                       nanovdb::CheckMode::Full,
                       false)) {
        throw std::runtime_error("merged NanoVDB output failed full validation");
    }
    nanovdb::io::writeGrid(
        arguments.output.string(),
        merged,
        nanovdb::io::Codec::NONE);
}

} // namespace

int main(int argc, char **argv)
{
    try {
        cook(parse_arguments(argc, argv));
        return 0;
    } catch (const std::exception &error) {
        std::cerr << error.what() << '\n';
        return 1;
    } catch (...) {
        std::cerr << "volume cooker failed with an unknown native exception\n";
        return 1;
    }
}
