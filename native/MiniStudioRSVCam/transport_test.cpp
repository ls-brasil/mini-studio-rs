#include "FrameReader.h"
#include "FrameWriter.h"
#include "SharedFrame.h"
#include <vector>
#include <thread>
#include <atomic>
#include <stdio.h>

#define CHECK(condition) do { if (!(condition)) { fprintf(stderr, "Falha: %s (linha %d)\n", #condition, __LINE__); return 1; } } while (0)

int main()
{
    wchar_t folder[MAX_PATH], path[MAX_PATH];
    CHECK(GetTempPathW(MAX_PATH, folder));
    CHECK(GetTempFileNameW(folder, L"mrs", 0, path));
    FrameWriter writer, duplicate;
    FrameReader reader;
    std::vector<uint8_t> source(MSVCAM_FRAME_BYTES, 42), output(MSVCAM_FRAME_BYTES);
    CHECK(SUCCEEDED(writer.Open(path)));
    CHECK(FAILED(duplicate.Open(path)));
    CHECK(reader.Open(path));
    CHECK(!reader.ReadLatest(output.data(), static_cast<uint32_t>(output.size())));
    CHECK(FAILED(writer.Publish(source.data(), 1)));
    CHECK(SUCCEEDED(writer.Publish(source.data(), static_cast<uint32_t>(source.size()))));
    CHECK(reader.ReadLatest(output.data(), static_cast<uint32_t>(output.size())));
    CHECK(source == output);
    // Leitura concorrente nunca pode misturar dois quadros.
    std::atomic<bool> done{false}, valid{true};
    std::thread producer([&] {
        for (int n=0; n<120; ++n) {
            memset(source.data(), n, source.size());
            if (FAILED(writer.Publish(source.data(), static_cast<uint32_t>(source.size())))) valid=false;
        }
        done=true;
    });
    while (!done) {
        if (reader.ReadLatest(output.data(), static_cast<uint32_t>(output.size()))) {
            const auto first=output.front();
            for (auto value : output) if (value!=first) valid=false;
        }
    }
    producer.join(); CHECK(valid);
    Sleep(550);
    CHECK(!reader.ReadLatest(output.data(), static_cast<uint32_t>(output.size())));
    writer.Close();
    CHECK(!reader.ReadLatest(output.data(), static_cast<uint32_t>(output.size())));
    CHECK(SUCCEEDED(writer.Open(path)));
    CHECK(SUCCEEDED(writer.Publish(source.data(), static_cast<uint32_t>(source.size()))));
    CHECK(reader.ReadLatest(output.data(), static_cast<uint32_t>(output.size())));
    reader.Close(); writer.Close();
    CHECK(DeleteFileW(path));
    puts("Transporte: exclusividade, integridade concorrente, heartbeat e reinicio OK.");
    return 0;
}
