#!/usr/bin/env bash
# The product tour's Claude Code (scripts/tour-capture.sh, GA-148): fake, like crates/gizai-agents/tests/fake-claude.sh,
# but its run reads like real work on the CSV export: it reads files, edits the exporter, runs the tests and then keeps
# working (it waits until Gizai stops it), so the board and the run show an agent at work. Asked for the model list, it
# answers like the tests' fake.
here="$(cd "$(dirname "$0")/../../crates/gizai-agents/tests/fixtures" && pwd)"
case " $* " in *" --input-format stream-json "*)
  IFS= read -r _request
  cat "$here/models-init.jsonl"
  cat > /dev/null
  exit 0 ;;
esac
case " $* " in *" --version "*|*" -v "*) echo "2.1.300 (Claude Code)"; exit 0 ;; esac
cat > /dev/null   # the prompt
trap 'exit 130' INT
say() { printf '%s\n' "$1"; sleep "${2:-0.15}"; }
say '{"type":"system","subtype":"init","session_id":"T1","model":"claude-opus-5-5","tools":["Read","Edit","Write","Bash","Grep"]}'
say '{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"I will add the CSV export to the invoices table, with semicolons as Marloes asked. First the exporter that is there."},{"type":"tool_use","id":"t1","name":"Grep","input":{"pattern":"Exporter","path":"app"}}]},"session_id":"T1"}'
say '{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"app/Exports/InvoiceExporter.php\napp/Filament/Resources/InvoiceResource.php","is_error":false}]},"session_id":"T1"}'
say '{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"t2","name":"Read","input":{"file_path":"app/Exports/InvoiceExporter.php"}}]},"session_id":"T1"}'
say '{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t2","content":"<?php\n\nnamespace App\\Exports;\n\nuse Filament\\Actions\\Exports\\Exporter;\n\nclass InvoiceExporter extends Exporter\n{\n}","is_error":false}]},"session_id":"T1"}'
say '{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"The exporter has no columns yet. I add number, date, client and amount, and set the delimiter to a semicolon."},{"type":"tool_use","id":"t3","name":"Edit","input":{"file_path":"app/Exports/InvoiceExporter.php","old_string":"{\n}","new_string":"{\n    protected static ?string $model = Invoice::class;\n\n    public static function getColumns(): array\n    {\n        return [\n            ExportColumn::make(\"number\"),\n            ExportColumn::make(\"date\"),\n            ExportColumn::make(\"client.name\"),\n            ExportColumn::make(\"amount\"),\n        ];\n    }\n\n    public function getCsvDelimiter(): string\n    {\n        return \";\";\n    }\n}"}}]},"session_id":"T1"}'
edit() { mkdir -p "$(dirname "$1")" && printf '%s\n' "$2" > "$1"; }
edit app/Exports/InvoiceExporter.php '<?php

namespace App\Exports;

use App\Models\Invoice;
use Filament\Actions\Exports\ExportColumn;
use Filament\Actions\Exports\Exporter;

class InvoiceExporter extends Exporter
{
    protected static ?string $model = Invoice::class;

    public static function getColumns(): array
    {
        return [
            ExportColumn::make("number"),
            ExportColumn::make("date"),
            ExportColumn::make("client.name"),
            ExportColumn::make("amount"),
        ];
    }

    public function getCsvDelimiter(): string
    {
        return ";";
    }
}'
say '{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t3","content":"The file app/Exports/InvoiceExporter.php has been updated.","is_error":false}]},"session_id":"T1"}'
say '{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"t4","name":"Edit","input":{"file_path":"app/Filament/Resources/InvoiceResource.php","old_string":"->headerActions([])","new_string":"->headerActions([\n    ExportAction::make()->exporter(InvoiceExporter::class),\n])"}}]},"session_id":"T1"}'
say '{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t4","content":"The file app/Filament/Resources/InvoiceResource.php has been updated.","is_error":false}]},"session_id":"T1"}'
say '{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Now a test that exports two invoices and reads the file back."},{"type":"tool_use","id":"t5","name":"Write","input":{"file_path":"tests/Feature/InvoiceExportTest.php","content":"<?php\n\nit(\"exports invoices with semicolons\", function () {\n    // two invoices, one export, the file read back\n});"}}]},"session_id":"T1"}'
edit tests/Feature/InvoiceExportTest.php '<?php

use App\Models\Invoice;

it("exports invoices with semicolons", function () {
    Invoice::factory()->count(2)->create();

    $csv = exportInvoices();

    expect($csv)->toContain(";")->and(substr_count($csv, "\n"))->toBe(3);
});'
say '{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t5","content":"File created successfully at: tests/Feature/InvoiceExportTest.php","is_error":false}]},"session_id":"T1"}'
say '{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"t6","name":"Bash","input":{"command":"php artisan test --filter=InvoiceExport","description":"Run the export tests"}}]},"session_id":"T1"}'
say '{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t6","content":"   PASS  Tests\\Feature\\InvoiceExportTest\n  ✓ exports invoices with semicolons                0.41s\n\n  Tests:    1 passed (6 assertions)\n  Duration: 0.58s","is_error":false}]},"session_id":"T1"}'
say '{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"The test passes. Next: the amounts with a comma, the way Kade books them, and the date as dd-mm-yyyy."},{"type":"tool_use","id":"t7","name":"Read","input":{"file_path":"storage/app/invoices-sample.csv"}}]},"session_id":"T1"}' 0
# still working: Gizai stops it (and the capture ends Gizai)
exec perl -e 'sleep 600'
