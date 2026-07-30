#include "wrapper.h"

#include "slang-com-ptr.h"
#include "slang.h"

#include <cstdlib>
#include <cstring>
#include <new>
#include <string>

namespace {

using Slang::ComPtr;

constexpr uint8_t SPIRV_MAGIC[] = {0x03, 0x02, 0x23, 0x07};

bool valid_options(const BFShaderCompilerOptions &options) {
    return options.stage >= BF_SHADER_STAGE_VERTEX
        && options.stage <= BF_SHADER_STAGE_COMPUTE
        && options.optimization >= SLANG_OPTIMIZATION_LEVEL_NONE
        && options.optimization <= SLANG_OPTIMIZATION_LEVEL_MAXIMAL
        && options.debug_info >= SLANG_DEBUG_INFO_LEVEL_NONE
        && options.debug_info <= SLANG_DEBUG_INFO_LEVEL_MAXIMAL;
}

SlangStage shader_stage(int32_t stage) {
    switch (stage) {
    case BF_SHADER_STAGE_VERTEX:
        return SLANG_STAGE_VERTEX;
    case BF_SHADER_STAGE_FRAGMENT:
        return SLANG_STAGE_FRAGMENT;
    case BF_SHADER_STAGE_COMPUTE:
        return SLANG_STAGE_COMPUTE;
    default:
        return SLANG_STAGE_NONE;
    }
}

bool copy_bytes(
    const void *data,
    size_t size,
    BFShaderCompilerBlob *output) {
    output->data = nullptr;
    output->size = 0;
    if (size == 0) {
        return true;
    }
    if (data == nullptr) {
        return false;
    }
    auto *copy = static_cast<uint8_t *>(std::malloc(size));
    if (copy == nullptr) {
        return false;
    }
    std::memcpy(copy, data, size);
    output->data = copy;
    output->size = size;
    return true;
}

bool copy_string(
    const std::string &text,
    BFShaderCompilerBlob *output) {
    return copy_bytes(text.data(), text.size(), output);
}

void append_diagnostics(
    std::string &destination,
    slang::IBlob *diagnostics) {
    if (diagnostics == nullptr || diagnostics->getBufferSize() == 0) {
        return;
    }
    if (!destination.empty() && destination.back() != '\n') {
        destination.push_back('\n');
    }
    const auto *text =
        static_cast<const char *>(diagnostics->getBufferPointer());
    destination.append(text, diagnostics->getBufferSize());
    while (!destination.empty() && destination.back() == '\0') {
        destination.pop_back();
    }
}

int32_t finish_failure(
    int32_t status,
    const std::string &diagnostics,
    BFShaderCompilerBlob *out_diagnostics) {
    if (!copy_string(diagnostics, out_diagnostics)) {
        return BF_SHADER_COMPILER_STATUS_OUT_OF_MEMORY;
    }
    return status;
}

int32_t compile_spirv(
    const uint8_t *source_name,
    size_t source_name_size,
    const uint8_t *source,
    size_t source_size,
    const uint8_t *entry_point,
    size_t entry_point_size,
    const BFShaderCompilerOptions &options,
    BFShaderCompilerBlob *out_spirv,
    BFShaderCompilerBlob *out_diagnostics) {
    std::string source_name_text(
        reinterpret_cast<const char *>(source_name),
        source_name_size);
    std::string source_text(
        reinterpret_cast<const char *>(source),
        source_size);
    std::string entry_point_text(
        reinterpret_cast<const char *>(entry_point),
        entry_point_size);
    if (source_name_text.find('\0') != std::string::npos
        || source_text.find('\0') != std::string::npos
        || entry_point_text.find('\0') != std::string::npos) {
        return BF_SHADER_COMPILER_STATUS_INVALID_ARGUMENT;
    }

    std::string diagnostics_text;
    ComPtr<slang::IGlobalSession> global_session;
    SlangResult result =
        slang::createGlobalSession(global_session.writeRef());
    if (SLANG_FAILED(result) || global_session == nullptr) {
        return finish_failure(
            BF_SHADER_COMPILER_STATUS_INITIALIZATION_FAILED,
            "could not create the Slang global session",
            out_diagnostics);
    }

    slang::CompilerOptionEntry target_options[2] = {};
    target_options[0].name = slang::CompilerOptionName::Optimization;
    target_options[0].value.intValue0 = options.optimization;
    target_options[1].name = slang::CompilerOptionName::DebugInformation;
    target_options[1].value.intValue0 = options.debug_info;

    slang::TargetDesc target = {};
    target.format = SLANG_SPIRV;
    target.profile = global_session->findProfile("spirv_1_5");
    target.compilerOptionEntries = target_options;
    target.compilerOptionEntryCount = 2;

    slang::CompilerOptionEntry session_option = {};
    session_option.name = slang::CompilerOptionName::WarningsAsErrors;
    session_option.value.kind =
        slang::CompilerOptionValueKind::String;
    session_option.value.stringValue0 = "all";

    slang::SessionDesc session_description = {};
    session_description.targets = &target;
    session_description.targetCount = 1;
    session_description.compilerOptionEntries = &session_option;
    session_description.compilerOptionEntryCount = 1;
    session_description.skipSPIRVValidation = false;

    ComPtr<slang::ISession> session;
    result = global_session->createSession(
        session_description,
        session.writeRef());
    if (SLANG_FAILED(result) || session == nullptr) {
        return finish_failure(
            BF_SHADER_COMPILER_STATUS_INITIALIZATION_FAILED,
            "could not create the Slang compilation session",
            out_diagnostics);
    }

    ComPtr<slang::IBlob> diagnostics;
    slang::IModule *module = session->loadModuleFromSourceString(
        "blackflower_asset",
        source_name_text.c_str(),
        source_text.c_str(),
        diagnostics.writeRef());
    append_diagnostics(diagnostics_text, diagnostics);
    if (module == nullptr) {
        return finish_failure(
            BF_SHADER_COMPILER_STATUS_COMPILATION_FAILED,
            diagnostics_text,
            out_diagnostics);
    }
    if (module->getDependencyFileCount() > 1) {
        return finish_failure(
            BF_SHADER_COMPILER_STATUS_COMPILATION_FAILED,
            "shader imports and includes are not supported",
            out_diagnostics);
    }

    ComPtr<slang::IEntryPoint> entry;
    diagnostics.setNull();
    result = module->findAndCheckEntryPoint(
        entry_point_text.c_str(),
        shader_stage(options.stage),
        entry.writeRef(),
        diagnostics.writeRef());
    append_diagnostics(diagnostics_text, diagnostics);
    if (SLANG_FAILED(result) || entry == nullptr) {
        return finish_failure(
            BF_SHADER_COMPILER_STATUS_COMPILATION_FAILED,
            diagnostics_text,
            out_diagnostics);
    }

    slang::IComponentType *components[] = {module, entry.get()};
    ComPtr<slang::IComponentType> program;
    diagnostics.setNull();
    result = session->createCompositeComponentType(
        components,
        2,
        program.writeRef(),
        diagnostics.writeRef());
    append_diagnostics(diagnostics_text, diagnostics);
    if (SLANG_FAILED(result) || program == nullptr) {
        return finish_failure(
            BF_SHADER_COMPILER_STATUS_COMPILATION_FAILED,
            diagnostics_text,
            out_diagnostics);
    }

    ComPtr<slang::IBlob> spirv;
    diagnostics.setNull();
    result = program->getEntryPointCode(
        0,
        0,
        spirv.writeRef(),
        diagnostics.writeRef());
    append_diagnostics(diagnostics_text, diagnostics);
    if (SLANG_FAILED(result) || spirv == nullptr) {
        return finish_failure(
            BF_SHADER_COMPILER_STATUS_COMPILATION_FAILED,
            diagnostics_text,
            out_diagnostics);
    }

    const size_t spirv_size = spirv->getBufferSize();
    const void *spirv_data = spirv->getBufferPointer();
    if (spirv_size < sizeof(SPIRV_MAGIC)
        || spirv_size % sizeof(uint32_t) != 0
        || spirv_data == nullptr
        || std::memcmp(spirv_data, SPIRV_MAGIC, sizeof(SPIRV_MAGIC)) != 0) {
        return finish_failure(
            BF_SHADER_COMPILER_STATUS_COMPILATION_FAILED,
            "Slang returned malformed SPIR-V",
            out_diagnostics);
    }
    if (!copy_bytes(spirv_data, spirv_size, out_spirv)
        || !copy_string(diagnostics_text, out_diagnostics)) {
        std::free(out_spirv->data);
        out_spirv->data = nullptr;
        out_spirv->size = 0;
        return BF_SHADER_COMPILER_STATUS_OUT_OF_MEMORY;
    }
    return BF_SHADER_COMPILER_STATUS_OK;
}

} // namespace

extern "C" const char *bf_shader_compiler_slang_version() {
    return BF_SLANG_VERSION;
}

extern "C" int32_t bf_shader_compiler_compile_spirv(
    const uint8_t *source_name,
    size_t source_name_size,
    const uint8_t *source,
    size_t source_size,
    const uint8_t *entry_point,
    size_t entry_point_size,
    const BFShaderCompilerOptions *options,
    BFShaderCompilerBlob *out_spirv,
    BFShaderCompilerBlob *out_diagnostics) {
    if (source_name == nullptr || source == nullptr || entry_point == nullptr
        || options == nullptr || out_spirv == nullptr
        || out_diagnostics == nullptr) {
        return BF_SHADER_COMPILER_STATUS_NULL_POINTER;
    }
    out_spirv->data = nullptr;
    out_spirv->size = 0;
    out_diagnostics->data = nullptr;
    out_diagnostics->size = 0;
    if (source_name_size == 0 || source_size == 0 || entry_point_size == 0
        || !valid_options(*options)) {
        return BF_SHADER_COMPILER_STATUS_INVALID_ARGUMENT;
    }

    try {
        return compile_spirv(
            source_name,
            source_name_size,
            source,
            source_size,
            entry_point,
            entry_point_size,
            *options,
            out_spirv,
            out_diagnostics);
    } catch (const std::bad_alloc &) {
        return BF_SHADER_COMPILER_STATUS_OUT_OF_MEMORY;
    } catch (...) {
        return finish_failure(
            BF_SHADER_COMPILER_STATUS_COMPILATION_FAILED,
            "unexpected native Slang compiler failure",
            out_diagnostics);
    }
}

extern "C" void bf_shader_compiler_blob_free(void *data) {
    std::free(data);
}
