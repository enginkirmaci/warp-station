using System;
using System.Text;
using System.Text.RegularExpressions;
using Tools.Library.Extensions;

namespace Tools.Library.Formatters
{
    public static class SqlToCSharpFormatter
    {
        public static string FormatCreateTableToClass(string createTableSql)
        {
            var tableNameMatch = Regex.Match(createTableSql, @"CREATE TABLE ""?(\w+)""?\.?""?(\w+)""?");
            if (!tableNameMatch.Success)
            {
                throw new ArgumentException("Invalid CREATE TABLE SQL format.");
            }

            var schema = tableNameMatch.Groups[1].Value;
            var tableName = tableNameMatch.Groups[2].Value;
            var className = tableName.ToPascalCase();

            var classBuilder = new StringBuilder();
            classBuilder.AppendLine($"[Table(\"{tableName}\", Schema = \"{schema}\")]");
            classBuilder.AppendLine($"public class {className} : BaseEntity");
            classBuilder.AppendLine("{");
            classBuilder.AppendLine("    #region Properties");
            classBuilder.AppendLine();

            var columnMatches = Regex.Matches(createTableSql, @"^\s*(?!CREATE TABLE)""?(\w+)""?\s+(\w+)(\(\d+(,\d+)?\))?", RegexOptions.Multiline);
            foreach (Match columnMatch in columnMatches)
            {
                var columnName = columnMatch.Groups[1].Value.ToPascalCase();
                var columnType = columnMatch.Groups[2].Value.ToCSharpType();

                classBuilder.AppendLine($"    public {columnType} {columnName} {{ get; set; }}");
                classBuilder.AppendLine();
            }

            classBuilder.AppendLine("    #endregion Properties");
            classBuilder.AppendLine("}");

            return classBuilder.ToString();
        }

        public static string GetTableName(string createTableSql)
        {
            var tableNameMatch = Regex.Match(createTableSql, @"CREATE TABLE ""?(\w+)""?\.?""?(\w+)""?");
            if (!tableNameMatch.Success)
            {
                throw new ArgumentException("Invalid CREATE TABLE SQL format.");
            }

            return tableNameMatch.Groups[2].Value.ToPascalCase();
        }

        private static string ToCSharpType(this string sqlType)
        {
            return sqlType switch
            {
                "VARCHAR2" => "string",
                "NUMBER" => "long?",
                "DATE" => "DateTime?",
                "TIMESTAMP" => "DateTime?",
                "CHAR" => "char?",
                "CLOB" => "string",
                "BLOB" => "byte[]",
                _ => "string"
            };
        }
    }
}